import { MeshJobs } from "../../components/dashboard/mesh-jobs";
import Layout from "../../components/layout";

export default function MeshJobsPage() {
  return (
    <Layout>
      <div className="space-y-6">
        <div>
          <h1 className="text-3xl font-bold tracking-tight">Mesh Compute Jobs</h1>
          <p className="text-muted-foreground mt-2">
            Monitor and manage distributed computation jobs across the planetary mesh network.
          </p>
        </div>
        <div className="space-y-6">
          <MeshJobs />
        </div>
      </div>
    </Layout>
  );
} 