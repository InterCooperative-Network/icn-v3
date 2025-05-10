#!/usr/bin/env node

/**
 * ICN v3 Organization-Scoped WebSocket Client
 * 
 * This tool allows connecting to ICN WebSocket channels with organization scoping.
 * It can be used for testing, debugging, and demonstrations.
 * 
 * Usage:
 *   node ws-client.js --url ws://localhost:8787/ws --federation-id fed1 --coop-id coop1 --community-id comm1
 */

const WebSocket = require('ws');
const chalk = require('chalk');
const yargs = require('yargs/yargs');
const { hideBin } = require('yargs/helpers');

// Parse command-line arguments
const argv = yargs(hideBin(process.argv))
  .option('url', {
    alias: 'u',
    description: 'Base WebSocket URL',
    default: 'ws://localhost:8787/ws',
    type: 'string',
  })
  .option('federation-id', {
    alias: 'f',
    description: 'Federation ID to scope the WebSocket channel',
    type: 'string',
  })
  .option('coop-id', {
    alias: 'c',
    description: 'Cooperative ID to scope the WebSocket channel',
    type: 'string',
  })
  .option('community-id', {
    alias: 'm',
    description: 'Community ID to scope the WebSocket channel',
    type: 'string',
  })
  .option('token', {
    alias: 't',
    description: 'JWT token for authentication',
    type: 'string',
  })
  .option('color-events', {
    description: 'Color-code different event types',
    default: true,
    type: 'boolean',
  })
  .option('format', {
    description: 'Output format (json, pretty)',
    default: 'pretty',
    choices: ['json', 'pretty'],
    type: 'string',
  })
  .help()
  .alias('help', 'h')
  .version('1.0.0')
  .alias('version', 'v')
  .argv;

// Build the WebSocket URL with query parameters
function buildWebSocketUrl() {
  const url = new URL(argv.url);
  
  if (argv.federationId) {
    url.searchParams.append('federation_id', argv.federationId);
  }
  
  if (argv.coopId) {
    url.searchParams.append('coop_id', argv.coopId);
  }
  
  if (argv.communityId) {
    url.searchParams.append('community_id', argv.communityId);
  }
  
  if (argv.token) {
    url.searchParams.append('token', argv.token);
  }
  
  return url.toString();
}

// Color event type based on type
function colorEventType(type) {
  if (!argv.colorEvents) return type;
  
  switch (type) {
    case 'ReceiptCreated':
      return chalk.blue(type);
    case 'TokenTransferred':
      return chalk.green(type);
    case 'TokenMinted':
      return chalk.magenta(type);
    case 'TokenBurned':
      return chalk.red(type);
    default:
      return chalk.yellow(type);
  }
}

// Connect to WebSocket and handle events
function connectWebSocket() {
  const url = buildWebSocketUrl();
  console.log(`Connecting to ${url}...`);
  
  const ws = new WebSocket(url);
  
  ws.on('open', () => {
    console.log(chalk.green('Connected! Waiting for events...'));
    console.log('Press Ctrl+C to disconnect');
    
    // Display channel info
    console.log('\nChannel Information:');
    console.log(`  Federation: ${argv.federationId || 'global'}`);
    console.log(`  Cooperative: ${argv.coopId || 'all'}`);
    console.log(`  Community: ${argv.communityId || 'all'}`);
    console.log('');
  });
  
  ws.on('message', (data) => {
    try {
      const event = JSON.parse(data);
      
      if (argv.format === 'json') {
        console.log(JSON.stringify(event));
      } else {
        // Pretty print the event
        const timestamp = new Date().toISOString();
        console.log(`[${chalk.gray(timestamp)}] Event: ${colorEventType(event.type)}`);
        
        if (event.data) {
          // Extract and display organization scope
          const orgScope = [];
          if (event.data.coop_id) orgScope.push(`coop:${event.data.coop_id}`);
          if (event.data.community_id) orgScope.push(`comm:${event.data.community_id}`);
          
          if (orgScope.length > 0) {
            console.log(`  Scope: ${chalk.cyan(orgScope.join(', '))}`);
          }
          
          // Handle different event types
          switch (event.type) {
            case 'ReceiptCreated':
              console.log(`  CID: ${event.data.cid}`);
              console.log(`  Executor: ${event.data.executor}`);
              console.log(`  Resources: ${JSON.stringify(event.data.resource_usage)}`);
              break;
              
            case 'TokenTransferred':
            case 'TokenMinted':
            case 'TokenBurned':
              console.log(`  TX ID: ${event.data.id}`);
              console.log(`  From: ${event.data.from_did}`);
              console.log(`  To: ${event.data.to_did}`);
              console.log(`  Amount: ${event.data.amount}`);
              break;
              
            default:
              console.log(JSON.stringify(event.data, null, 2));
          }
          
          console.log('');  // Add empty line between events
        }
      }
    } catch (err) {
      console.error(chalk.red('Error parsing message:'), err);
      console.log('Raw message:', data);
    }
  });
  
  ws.on('error', (error) => {
    console.error(chalk.red('WebSocket error:'), error.message);
  });
  
  ws.on('close', (code, reason) => {
    console.log(chalk.yellow(`Connection closed: ${code} ${reason}`));
    process.exit(0);
  });
  
  // Handle Ctrl+C to close the connection gracefully
  process.on('SIGINT', () => {
    console.log(chalk.yellow('\nDisconnecting...'));
    ws.close();
  });
}

// Start the client
connectWebSocket(); 