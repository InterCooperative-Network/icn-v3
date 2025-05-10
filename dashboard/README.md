# ICN Dashboard

A modern, responsive dashboard for monitoring and managing the ICN (Inter-Cranial Network) distributed computing network.

## Features

- **Real-time Monitoring**: View the latest execution receipts, node status, and token balances
- **Federation Status**: Monitor the health and capabilities of federation nodes
- **Receipt Explorer**: Search and browse execution receipts anchored in the DAG
- **Token Ledger**: Track token balances and economics metrics
- **Visual Analytics**: Interactive charts for receipt and token statistics
- **Detail Drill-Down**: Click on chart elements to explore filtered data views
- **Governance Interface**: View and vote on governance proposals (coming soon)

## Tech Stack

- **Next.js**: React framework for server-rendered applications
- **TypeScript**: Type safety for better developer experience
- **Tailwind CSS**: Utility-first CSS framework
- **Axios**: Promise-based HTTP client for API requests
- **Recharts**: Composable charting library for data visualization

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
```

## Connecting to ICN Runtime

By default, the dashboard connects to an ICN runtime at `http://localhost:8080`. You can change this by setting the `NEXT_PUBLIC_API_URL` environment variable.

If the API is not available, the dashboard will fall back to mock data for demonstration purposes.

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

## License

Apache 2.0
