"use client";
import Sidebar from "@/components/Sidebar";
import SearchBar from "@/components/SearchBar";

export default function DashboardLayout({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex min-h-screen">
      <Sidebar />
      <main className="ml-60 flex-1 p-6 overflow-auto">
        <div className="mb-6">
          <SearchBar />
        </div>
        {children}
      </main>
    </div>
  );
}
