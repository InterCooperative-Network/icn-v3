import { Grid, Paper, Typography, Container } from '@mui/material';
import Layout from '../components/Layout';
import ReputationLeaderboard from '../components/ReputationLeaderboard';
import ReputationActivityChart from '../components/ReputationActivityChart';

export default function ReputationPage() {
  return (
    <Layout>
      <Container maxWidth="lg" sx={{ mt: 4, mb: 4 }}>
        <Typography variant="h4" gutterBottom>
          Reputation
        </Typography>
        
        <Grid container spacing={3}>
          {/* Reputation Activity Chart */}
          <Grid item xs={12}>
            <ReputationActivityChart />
          </Grid>
          
          {/* Reputation Leaderboard */}
          <Grid item xs={12}>
            <Paper sx={{ p: 2, display: 'flex', flexDirection: 'column' }}>
              <ReputationLeaderboard />
            </Paper>
          </Grid>
        </Grid>
      </Container>
    </Layout>
  );
} 