import { NextResponse } from "next/server";
import type { NextRequest } from "next/server";

const AUTH_COOKIES = [
  "token",
  "zaps-auth",
  "privy-token",
  "privy-id-token",
  "privy-session",
];

function isAuthenticated(request: NextRequest): boolean {
  return AUTH_COOKIES.some((name) => Boolean(request.cookies.get(name)?.value));
}

export function middleware(request: NextRequest) {
  if (isAuthenticated(request)) {
    return NextResponse.next();
  }

  return NextResponse.redirect(new URL("/login", request.url));
}

export const config = {
  matcher: ["/dashboard", "/dashboard/:path*"],
};
