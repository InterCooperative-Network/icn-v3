# ICN Dashboard

A modern, responsive dashboard for monitoring and managing the ICN (Inter-Cranial Network) distributed computing network.

## Features

- **Real-time Monitoring**: View the latest execution receipts, node status, and token balances
- **Federation Status**: Monitor the health and capabilities of federation nodes
- **Receipt Explorer**: Search and browse execution receipts anchored in the DAG
- **Token Ledger**: Track token balances and economics metrics
- **Visual Analytics**: Interactive charts for receipt and token statistics
- **Detail Drill-Down**: Click on chart elements to explore filtered data views
- **Live Updates**: WebSocket integration for real-time data streaming
- **Federation-Aware**: Scope data visibility by federation with secure channels
- **Governance Interface**: View and vote on governance proposals (coming soon)

## Tech Stack

- **Next.js**: React framework for server-rendered applications
- **TypeScript**: Type safety for better developer experience
- **Tailwind CSS**: Utility-first CSS framework
- **Recharts**: Composable charting library for data visualization
- **Socket.IO**: Real-time WebSocket communication
- **Axios**: Promise-based HTTP client for API requests

## Getting Started

### Prerequisites

- Node.js 18.x or later
- npm or yarn

### Installation

1. Clone the repository
   ```
   git clone https://github.com/yourusername/icn-v3.git
   cd icn-v3/dashboard
   ```

2. Install dependencies
   ```
   npm install
   # or
   yarn install
   ```

3. Start the development server
   ```
   npm run dev
   # or
   yarn dev
   ```

4. Open [http://localhost:3000](http://localhost:3000) with your browser to see the result.

## Environment Variables

Create a `.env.local` file in the dashboard directory:

```
NEXT_PUBLIC_API_URL=http://localhost:8080
NEXT_PUBLIC_SOCKET_URL=http://localhost:8081
```

## Connecting to ICN Runtime

By default, the dashboard connects to an ICN runtime at `http://localhost:8080` for REST API calls and `http://localhost:8081` for WebSocket events. You can change these by setting the appropriate environment variables.

If the API is not available, the dashboard will fall back to mock data for demonstration purposes.

## Real-time Updates

The dashboard includes WebSocket integration for live updates:

- **Receipt Creation**: Receipt charts update automatically when new receipts are created
- **Token Transactions**: Token balance and history charts update on mints, burns, and transfers
- **Federation Nodes**: Node status indicators update when nodes go online or offline
- **Federation Scoping**: Data is scoped to the selected federation for security

To test the basic real-time features locally, you can use the included WebSocket server example:

```
npm install socket.io express
node websocket-server-example.js
```

For federation-scoped WebSockets with authentication:

```
npm install socket.io express jsonwebtoken
node websocket-server-example-federated.js
```

## Federation Support

The dashboard supports multiple federations:

- **Federation Selector**: Switch between federations or view all
- **Scoped Data**: Data is filtered by federation automatically
- **Authenticated Channels**: Secure access to federation-specific data
- **Federation Namespaces**: Separate Socket.IO namespaces per federation

## Interactive Features

The dashboard includes interactive charts that allow operators to drill down into specific data:

- **Receipt Charts**: 
  - Click on a date point to view all receipts from that day
  - Click on an executor bar to view all receipts from that executor

- **Token Charts**:
  - Click on a date in the supply history to view token activity for that day
  - Click on a segment in the distribution pie chart to view details for that account

## Development

### Folder Structure

- `app/`: Next.js app router pages
- `components/`: Reusable UI components
- `components/dashboard/`: Dashboard-specific components
- `components/ui/`: Generic UI components
- `lib/`: Utility functions, API clients, and types

### Adding New Pages

Create a new file in the `app` directory, e.g., `app/network/page.tsx`.

### API Integration

Update the API client in `lib/api.ts` to add new endpoints as needed.

### WebSocket Events

The real-time events are defined in `lib/realtime.ts`. To add new event types:

1. Add a new event type to the `RealtimeEvent` enum
2. Update the WebSocket server to emit events with the matching type
3. Use the `useRealtimeEvent` hook in your component to subscribe to the event

### Federation Support

To add a new federation:

1. Add the federation ID to the federated WebSocket server
2. Update the FederationSelector component with the new federation
3. Ensure your authentication system supports the new federation

## License

Apache 2.0
