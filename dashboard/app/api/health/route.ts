export async function GET() {
  return Response.json({
    rpc: "healthy",
    db: "healthy",
    timestamp: new Date().toISOString()
  });
}
