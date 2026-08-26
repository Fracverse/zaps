"use client";
import { useEffect } from "react";
import { useRouter } from "next/navigation";

export default function HomePage() {
  const router = useRouter();
  
  useEffect(() => {
    router.push("/dashboard");
  }, [router]);
  
  return (
    <div className="min-h-screen flex items-center justify-center bg-slate-50">
      <div className="text-center">
        <span className="text-3xl">⚡</span>
        <h1 className="text-xl font-bold text-slate-900 mt-2">Loading...</h1>
      </div>
    </div>
  );
}
