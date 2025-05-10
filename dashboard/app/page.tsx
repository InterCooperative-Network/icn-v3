import Layout from '../components/layout';
import { ReceiptStats } from '../components/dashboard/receipt-stats';
import { TokenStats } from '../components/dashboard/token-stats';
import { FederationStatus } from '../components/dashboard/federation-status';
import { ReceiptCharts } from '../components/dashboard/receipt-charts';
import { TokenCharts } from '../components/dashboard/token-charts';

export default function Home() {
  return (
    <Layout>
      <div className="space-y-6">
        <h1 className="text-3xl font-bold">ICN Dashboard</h1>
        <p className="text-slate-600">
          Monitor your ICN federation, track execution receipts, and manage governance proposals.
        </p>
        
        <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
          <ReceiptStats />
          <div className="space-y-6">
            <TokenStats />
            <FederationStatus />
          </div>
        </div>
        
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6 mt-6">
          <ReceiptCharts />
          <TokenCharts />
        </div>
      </div>
    </Layout>
  );
}
