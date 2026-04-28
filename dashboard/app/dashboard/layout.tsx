"use client";
import { useEffect } from "react";
import { useRouter } from "next/navigation";
import { useAuth } from "@/lib/auth-context";
import Sidebar from "@/components/Sidebar";

export default function DashboardLayout({ children }: { children: React.ReactNode }) {
  const { token } = useAuth();
  const router = useRouter();

  useEffect(() => {
    if (token === null && typeof window !== "undefined" && !localStorage.getItem("token")) {
      router.replace("/login");
    }
  }, [token, router]);

  return (
    <div className="flex min-h-screen">
      <Sidebar />
      <main className="ml-60 flex-1 p-6 overflow-auto">{children}</main>
    </div>
  );
}
