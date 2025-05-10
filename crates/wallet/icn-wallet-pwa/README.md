# ICN Wallet PWA

A browser-based Progressive Web App wallet for the Internet of Cooperation Network (ICN).

## Features

- **Offline-First**: Works without an internet connection
- **Secure Identity Management**: Create and manage Ed25519 keypairs as DID:key identifiers
- **Local Storage**: Credentials stored securely in browser's IndexedDB
- **Organization Scoping**: Context-aware identity management for federation, cooperative, and community
- **Interoperability**: Exposes a JavaScript API for other apps to request signatures
- **Cross-Platform**: Works on desktop and mobile browsers

## Architecture

The ICN Wallet consists of several core components:

1. **Cryptography Module**: Implements Ed25519 key generation and signatures
2. **Storage Service**: Securely stores keys and credentials in IndexedDB
3. **Wallet API**: Exposes methods for external apps to request signatures
4. **PWA Shell**: Progressive Web App with offline support
5. **Identity Management UI**: User interface for managing identities and credentials

## Development

### Prerequisites

- Node.js 18.x or later (required by Next.js)
- npm or yarn
- OR Podman for containerized development

### Setup

#### Local Development (requires Node.js 18+)

```bash
# Install dependencies
cd crates/wallet/icn-wallet-pwa
npm install

# Start development server
npm run dev
```

The development server will start at http://localhost:3001.

#### Using Podman (no specific Node.js version needed)

If you don't want to install Node.js 18+ directly on your machine, you can use Podman:

```bash
# Make sure Podman is installed
# On Ubuntu/Debian: sudo apt-get install podman
# On Fedora: sudo dnf install podman

# Run the provided script
cd crates/wallet/icn-wallet-pwa
./run-podman.sh
```

The Podman script supports several options:

```bash
# Run in production mode
./run-podman.sh --mode prod

# Enable access from other devices on your network
./run-podman.sh --bind 0.0.0.0

# Run with HTTPS (generates self-signed certificates)
./run-podman.sh --https

# Run with custom port
./run-podman.sh --port 8443

# Generate systemd service file for persistent operation
./run-podman.sh --systemd

# Show all options
./run-podman.sh --help
```

This will build a container with Node.js 18 and start the development server.

### Building for Production

```bash
# Locally
npm run build

# With Podman
./run-podman.sh --mode prod
```

### Persistent Data Storage

When running with Podman, a persistent volume named `icn-wallet-pwa-data` is created to store application data. This ensures your identities and settings are preserved across container restarts.

You can specify a custom volume name:

```bash
./run-podman.sh --volume my-custom-volume
```

## Integration with ICN

The ICN Wallet integrates with other ICN components:

- **Runtime**: Signs and submits transactions
- **AgoraNet**: Authenticates governance participation
- **Dashboard**: Provides identity for viewing and interacting with federation data

## API

External applications can interact with the wallet through the JavaScript API:

```javascript
// Example of requesting a signature
const message = new TextEncoder().encode('Hello ICN');
const request = {
  id: 'unique-request-id',
  action: 'sign',
  params: {
    did: 'did:key:z...',
    message: Array.from(message)
  }
};

// Send the request via postMessage
window.postMessage(request, window.origin);

// Listen for the response
window.addEventListener('message', (event) => {
  const response = event.data;
  if (response.id === 'unique-request-id') {
    console.log('Signature:', response.data.signature);
  }
});
```

## Security Considerations

- The wallet uses IndexedDB for storage, which is isolated per origin
- Private keys never leave the browser
- Cross-origin communication is restricted to trusted origins
- All cryptographic operations are performed client-side
- When using HTTPS, secure communication is ensured

## License

Apache License 2.0 