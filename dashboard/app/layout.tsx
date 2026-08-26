"use client";
import "./globals.css";

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en">
      <body>
        <div className="min-h-screen bg-slate-50">
          {children}
        </div>
      </body>
    </html>
  );
}
