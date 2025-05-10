/**
 * Enhanced WebSocket Server with Federation Namespaces
 * ---------------------------------------------------
 * This is an example of a WebSocket server that supports federation-specific namespaces
 * and authenticated connections for the ICN Dashboard.
 * 
 * To use:
 * 1. Install dependencies: npm install socket.io express jsonwebtoken
 * 2. Run this file: node websocket-server-example-federated.js
 * 3. Set the dashboard env var: NEXT_PUBLIC_SOCKET_URL=http://localhost:8081
 */

const express = require('express');
const http = require('http');
const { Server } = require('socket.io');
// Note: For this example, you'd need to install jsonwebtoken: npm install jsonwebtoken
let jwt;
try {
  jwt = require('jsonwebtoken');
} catch (err) {
  console.warn('jsonwebtoken package not found. Running without JWT authentication.');
  // Simple mock implementation for the example to work without installing jsonwebtoken
  jwt = {
    verify: (token, secret) => {
      // In development, we'll accept any token and extract federation from it
      const [_, federationInfo] = token.split('.');
      try {
        return JSON.parse(Buffer.from(federationInfo, 'base64').toString());
      } catch (e) {
        return { did: 'anonymous', federationId: 'global' };
      }
    }
  };
}

const app = express();
const server = http.createServer(app);
const io = new Server(server, {
  cors: {
    origin: '*', // For development only - restrict in production
    methods: ['GET', 'POST']
  }
});

// JWT secret - should be environment variable in production
const JWT_SECRET = process.env.JWT_SECRET || 'development_secret';

// Middleware to verify JWT tokens for authenticated namespaces
const authenticateToken = (socket, next) => {
  const token = socket.handshake.auth?.token;
  
  if (!token) {
    return next(new Error('Authentication token required'));
  }
  
  try {
    // Verify the token
    const decoded = jwt.verify(token, JWT_SECRET);
    
    // Attach user info to socket
    socket.user = {
      did: decoded.did,
      federationId: decoded.federationId,
      roles: decoded.roles || []
    };
    
    // Check if user is authorized for this namespace
    const namespace = socket.nsp.name.substring(1); // Remove leading /
    
    if (namespace !== 'global' && namespace !== socket.user.federationId) {
      return next(new Error('Not authorized for this federation'));
    }
    
    next();
  } catch (err) {
    console.error('JWT verification error:', err);
    next(new Error('Invalid token'));
  }
};

// Sample federation data
const federations = ['global', 'federation1', 'federation2', 'federation3'];

// Create a basic endpoint to generate sample JWT tokens
app.get('/auth/:federation', (req, res) => {
  const federation = req.params.federation;
  if (!federations.includes(federation)) {
    return res.status(400).json({ error: 'Invalid federation' });
  }
  
  const token = createToken(federation);
  res.json({ token });
});

// Function to create JWT tokens for testing
function createToken(federationId) {
  // In a real app, this would include proper authentication
  const payload = {
    did: `did:icn:user-${Math.random().toString(36).substring(2, 10)}`,
    federationId,
    roles: ['viewer'],
    exp: Math.floor(Date.now() / 1000) + (60 * 60) // 1 hour expiration
  };
  
  try {
    return jwt.sign ? jwt.sign(payload, JWT_SECRET) : btoa(JSON.stringify(payload));
  } catch (err) {
    // If jwt.sign isn't available (no jsonwebtoken package), return a fake token
    return `fake.${Buffer.from(JSON.stringify(payload)).toString('base64')}.token`;
  }
}

// Create federation-specific namespaces
function createFederationNamespace(federationId) {
  const nsp = io.of(`/${federationId}`);
  
  // Apply auth middleware for non-global namespaces
  if (federationId !== 'global') {
    nsp.use(authenticateToken);
  }
  
  // Set up connection handling
  nsp.on('connection', (socket) => {
    console.log(`Client connected to federation: ${federationId}`);
    
    // Log authenticated user info if available
    if (socket.user) {
      console.log(`Authenticated user: ${socket.user.did}`);
    }
    
    socket.on('disconnect', () => {
      console.log(`Client disconnected from federation: ${federationId}`);
    });
  });
  
  return nsp;
}

// Create namespaces for each federation
const namespaces = {};
federations.forEach(federationId => {
  namespaces[federationId] = createFederationNamespace(federationId);
  console.log(`Created namespace for federation: ${federationId}`);
});

// Event types that match the frontend
const events = {
  RECEIPT_CREATED: 'receipt:created',
  TOKEN_TRANSFERRED: 'token:transferred',
  TOKEN_MINTED: 'token:minted',
  TOKEN_BURNED: 'token:burned',
  FEDERATION_NODE_STATUS: 'federation:node:status'
};

// Start the server
const PORT = process.env.PORT || 8081;
server.listen(PORT, () => {
  console.log(`Federation WebSocket server running on port ${PORT}`);
  console.log(`Auth token endpoint: http://localhost:${PORT}/auth/:federation`);
});

// ---- Sample event emitters for testing ----

// Emit receipt events for each federation
setInterval(() => {
  // Emit to specific federations
  Object.entries(namespaces).forEach(([federationId, namespace]) => {
    if (federationId !== 'global' && namespace.sockets.size > 0) {
      const receipt = generateMockReceipt(federationId);
      namespace.emit(events.RECEIPT_CREATED, receipt);
      console.log(`Emitted receipt to ${federationId}: ${receipt.task_cid}`);
      
      // Also emit to global namespace for aggregated view
      namespaces.global.emit(events.RECEIPT_CREATED, receipt);
    }
  });
}, 10000);

// Emit token events
setInterval(() => {
  // Emit token events to random federations
  const federationId = federations[Math.floor(Math.random() * federations.length)];
  if (federationId !== 'global' && namespaces[federationId].sockets.size > 0) {
    // Choose a random token event type
    const eventTypes = [
      events.TOKEN_TRANSFERRED,
      events.TOKEN_MINTED,
      events.TOKEN_BURNED
    ];
    const eventType = eventTypes[Math.floor(Math.random() * eventTypes.length)];
    
    const transaction = generateMockTokenTransaction(eventType, federationId);
    namespaces[federationId].emit(eventType, transaction);
    console.log(`Emitted ${eventType} to ${federationId}: ${transaction.id}`);
    
    // Also emit to global namespace
    namespaces.global.emit(eventType, transaction);
  }
}, 15000);

// Emit federation node status updates
setInterval(() => {
  if (namespaces.global.sockets.size > 0) {
    const federationId = federations.find(f => f !== 'global');
    const status = {
      nodeId: `node-${Math.floor(Math.random() * 10)}`,
      federationId: federationId,
      status: Math.random() > 0.5 ? 'online' : 'offline',
      timestamp: new Date().toISOString()
    };
    
    namespaces.global.emit(events.FEDERATION_NODE_STATUS, status);
    console.log(`Emitted node status update for ${status.nodeId}: ${status.status}`);
  }
}, 20000);

// ---- Mock data generators ----

// Generate a random CID
function randomCid() {
  const chars = 'abcdefghijklmnopqrstuvwxyz0123456789';
  let result = 'bafy';
  for (let i = 0; i < 16; i++) {
    result += chars.charAt(Math.floor(Math.random() * chars.length));
  }
  return result;
}

// Mock receipt generator with federation context
function generateMockReceipt(federationId) {
  return {
    task_cid: randomCid(),
    executor: `did:icn:${federationId}:${Math.random().toString(36).substr(2, 8)}`,
    federation_id: federationId,
    resource_usage: {
      CPU: Math.floor(Math.random() * 1000) + 100,
      Memory: Math.floor(Math.random() * 2048) + 256,
      Storage: Math.floor(Math.random() * 10000) + 1000
    },
    timestamp: new Date().toISOString(),
    signature: "0x" + Math.random().toString(16).substr(2, 16)
  };
}

// Mock token transaction generator with federation context
function generateMockTokenTransaction(eventType, federationId) {
  // Generate federation-specific accounts
  const accounts = [
    `did:icn:${federationId}:node1`, 
    `did:icn:${federationId}:node2`, 
    `did:icn:${federationId}:user1`, 
    `did:icn:${federationId}:user2`
  ];
  
  let from, to;
  
  switch (eventType) {
    case events.TOKEN_MINTED:
      from = 'did:icn:treasury';
      to = accounts[Math.floor(Math.random() * accounts.length)];
      break;
    case events.TOKEN_BURNED:
      from = accounts[Math.floor(Math.random() * accounts.length)];
      to = 'did:icn:treasury';
      break;
    case events.TOKEN_TRANSFERRED:
      from = accounts[Math.floor(Math.random() * accounts.length)];
      // Ensure from and to are different
      do {
        to = accounts[Math.floor(Math.random() * accounts.length)];
      } while (to === from);
      break;
  }
  
  return {
    id: `tx-${Date.now().toString(36)}`,
    from,
    to,
    amount: Math.floor(Math.random() * 1000) + 100,
    federation_id: federationId,
    operation: eventType.split(':')[1], // Get 'minted', 'burned', or 'transferred'
    timestamp: new Date().toISOString()
  };
} 