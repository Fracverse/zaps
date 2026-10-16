import { NextResponse } from "next/server";
import type { NextRequest } from "next/server";

const AUTH_COOKIES = [
  "token",
  "zaps-auth",
  "privy-token",
  "privy-id-token",
  "privy-session",
];

// Paths that require superadmin access
const SUPERADMIN_PATHS = [
  "/dashboard/contracts",
  "/dashboard/yield",
];

// Public paths that don't require authentication
const PUBLIC_PATHS = [
  "/login",
  "/api/health",
  "/api/public",
];

function isAuthenticated(request: NextRequest): boolean {
  return AUTH_COOKIES.some((name) => Boolean(request.cookies.get(name)?.value));
}

function isSuperadminPath(pathname: string): boolean {
  return SUPERADMIN_PATHS.some((path) => pathname.startsWith(path));
}

function isPublicPath(pathname: string): boolean {
  return PUBLIC_PATHS.some((path) => pathname.startsWith(path));
}

function hasSuperadminCookie(request: NextRequest): boolean {
  // Check for a superadmin role indicator (you may need to adjust this based on your auth implementation)
  const role = request.cookies.get("user_role")?.value;
  return role === "superadmin" || role === "admin";
}

export function middleware(request: NextRequest) {
  const { pathname } = request.nextUrl;

  // Skip middleware for public paths
  if (isPublicPath(pathname)) {
    return NextResponse.next();
  }

  // Check authentication
  if (!isAuthenticated(request)) {
    const loginUrl = new URL("/login", request.url);
    loginUrl.searchParams.set("redirect", pathname);
    return NextResponse.redirect(loginUrl);
  }

  // Check superadmin access for protected paths
  if (isSuperadminPath(pathname) && !hasSuperadminCookie(request)) {
    // Redirect to dashboard with an error message
    const dashboardUrl = new URL("/dashboard", request.url);
    dashboardUrl.searchParams.set("error", "access_denied");
    dashboardUrl.searchParams.set("message", "Superadmin access required");
    return NextResponse.redirect(dashboardUrl);
  }

  return NextResponse.next();
}

export const config = {
  matcher: [
    "/dashboard/:path*",
    "/api/:path*",
  ],
};
