/**
 * WebSocket Server Example for ICN Dashboard Real-time Updates
 * -------------------------------------------------------------
 * This is a simple example of a WebSocket server that can emit events to the ICN Dashboard.
 * 
 * To use:
 * 1. Install dependencies: npm install socket.io express
 * 2. Run this file: node websocket-server-example.js
 * 3. Set the dashboard env var: NEXT_PUBLIC_SOCKET_URL=http://localhost:8081
 */

const express = require('express');
const http = require('http');
const { Server } = require('socket.io');

const app = express();
const server = http.createServer(app);
const io = new Server(server, {
  cors: {
    origin: '*', // For development only - restrict in production
    methods: ['GET', 'POST']
  }
});

// Event types that match the frontend
const events = {
  RECEIPT_CREATED: 'receipt:created',
  TOKEN_TRANSFERRED: 'token:transferred',
  TOKEN_MINTED: 'token:minted',
  TOKEN_BURNED: 'token:burned',
  FEDERATION_NODE_STATUS: 'federation:node:status'
};

// Client connections
let connections = 0;

io.on('connection', (socket) => {
  connections++;
  console.log(`Client connected. Total connections: ${connections}`);
  
  socket.on('disconnect', () => {
    connections--;
    console.log(`Client disconnected. Total connections: ${connections}`);
  });
});

// Start the server
const PORT = process.env.PORT || 8081;
server.listen(PORT, () => {
  console.log(`WebSocket server running on port ${PORT}`);
});

// ---- Sample event emitters for testing ----

// Generates a mock receipt event every 10 seconds
setInterval(() => {
  if (connections > 0) {
    const receipt = generateMockReceipt();
    io.emit(events.RECEIPT_CREATED, receipt);
    console.log(`Emitted receipt event: ${receipt.task_cid}`);
  }
}, 10000);

// Generates a token transaction every 30 seconds
setInterval(() => {
  if (connections > 0) {
    // Choose a random token event type
    const eventTypes = [
      events.TOKEN_TRANSFERRED,
      events.TOKEN_MINTED,
      events.TOKEN_BURNED
    ];
    const eventType = eventTypes[Math.floor(Math.random() * eventTypes.length)];
    
    const transaction = generateMockTokenTransaction(eventType);
    io.emit(eventType, transaction);
    console.log(`Emitted ${eventType} event: ${transaction.id}`);
  }
}, 30000);

// ---- Mock data generators ----

// Generate a random DID
function randomDid() {
  const chars = 'abcdefghijklmnopqrstuvwxyz0123456789';
  let result = 'did:icn:';
  for (let i = 0; i < 10; i++) {
    result += chars.charAt(Math.floor(Math.random() * chars.length));
  }
  return result;
}

// Generate a random CID
function randomCid() {
  const chars = 'abcdefghijklmnopqrstuvwxyz0123456789';
  let result = 'bafy';
  for (let i = 0; i < 16; i++) {
    result += chars.charAt(Math.floor(Math.random() * chars.length));
  }
  return result;
}

// Mock receipt generator
function generateMockReceipt() {
  return {
    task_cid: randomCid(),
    executor: randomDid(),
    resource_usage: {
      CPU: Math.floor(Math.random() * 1000) + 100,
      Memory: Math.floor(Math.random() * 2048) + 256,
      Storage: Math.floor(Math.random() * 10000) + 1000
    },
    timestamp: new Date().toISOString(),
    signature: "0x" + Math.random().toString(16).substr(2, 16)
  };
}

// Mock token transaction generator
function generateMockTokenTransaction(eventType) {
  const accounts = [
    'did:icn:node1', 
    'did:icn:node2', 
    'did:icn:node3', 
    'did:icn:user1', 
    'did:icn:user2'
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
    operation: eventType.split(':')[1], // Get 'minted', 'burned', or 'transferred'
    timestamp: new Date().toISOString()
  };
} 