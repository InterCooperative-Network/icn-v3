"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { cn } from "../lib/utils";

interface LayoutProps {
  children: React.ReactNode;
}

const navigationItems = [
  {
    name: "Dashboard",
    href: "/",
    icon: (
      <svg
        xmlns="http://www.w3.org/2000/svg"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
        className="w-5 h-5"
      >
        <rect width="7" height="9" x="3" y="3" rx="1" />
        <rect width="7" height="5" x="14" y="3" rx="1" />
        <rect width="7" height="9" x="14" y="12" rx="1" />
        <rect width="7" height="5" x="3" y="16" rx="1" />
      </svg>
    ),
  },
  {
    name: "Receipts",
    href: "/receipts",
    icon: (
      <svg
        xmlns="http://www.w3.org/2000/svg"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
        className="w-5 h-5"
      >
        <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
        <path d="M14 2v6h6" />
        <path d="M16 13H8" />
        <path d="M16 17H8" />
        <path d="M10 9H8" />
      </svg>
    ),
  },
  {
    name: "Tokens",
    href: "/tokens",
    icon: (
      <svg
        xmlns="http://www.w3.org/2000/svg"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
        className="w-5 h-5"
      >
        <circle cx="12" cy="12" r="10" />
        <path d="M12 6v12" />
        <path d="M8 12h8" />
      </svg>
    ),
  },
  {
    name: "Governance",
    href: "/governance",
    icon: (
      <svg
        xmlns="http://www.w3.org/2000/svg"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
        className="w-5 h-5"
      >
        <path d="M19 14c1.49-1.46 3-3.21 3-5.5A5.5 5.5 0 0 0 16.5 3c-1.76 0-3 .5-4.5 2-1.5-1.5-2.74-2-4.5-2A5.5 5.5 0 0 0 2 8.5c0 2.3 1.5 4.05 3 5.5l7 7Z" />
      </svg>
    ),
  },
  {
    name: "Network",
    href: "/network",
    icon: (
      <svg
        xmlns="http://www.w3.org/2000/svg"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
        className="w-5 h-5"
      >
        <path d="M4 21V9a3 3 0 0 1 3-3h10a3 3 0 0 1 3 3v12" />
        <path d="M7 21h10" />
        <path d="M7.5 15h9" />
        <path d="M7.5 12h9" />
        <path d="M7.5 9h9" />
        <path d="M7.5 6h9" />
      </svg>
    ),
  },
];

export default function Layout({ children }: LayoutProps) {
  const pathname = usePathname();

  return (
    <div className="flex min-h-screen">
      {/* Sidebar */}
      <div className="hidden sm:flex w-64 flex-col bg-slate-900 text-white">
        <div className="flex h-16 items-center px-4 border-b border-slate-800">
          <h1 className="text-xl font-bold">ICN Dashboard</h1>
        </div>
        <nav className="flex-1 space-y-1 py-4">
          {navigationItems.map((item) => (
            <Link
              key={item.name}
              href={item.href}
              className={cn(
                "flex items-center px-4 py-2 text-sm font-medium",
                pathname === item.href
                  ? "bg-slate-800 text-white"
                  : "text-slate-300 hover:bg-slate-800 hover:text-white"
              )}
            >
              <div className="mr-3">{item.icon}</div>
              {item.name}
            </Link>
          ))}
        </nav>
        <div className="flex-shrink-0 p-4 border-t border-slate-800">
          <div className="flex items-center">
            <div className="ml-3">
              <p className="text-sm font-medium text-white">ICN v3</p>
              <p className="text-xs text-slate-300">Connected to federation</p>
            </div>
          </div>
        </div>
      </div>

      {/* Mobile header */}
      <div className="sm:hidden fixed top-0 left-0 right-0 z-10 flex items-center h-16 px-4 bg-slate-900 text-white border-b border-slate-800">
        <h1 className="text-xl font-bold">ICN Dashboard</h1>
      </div>

      {/* Main content */}
      <div className="flex-1 overflow-auto">
        <main className="p-6 sm:p-8 mt-16 sm:mt-0">{children}</main>
      </div>
    </div>
  );
} 