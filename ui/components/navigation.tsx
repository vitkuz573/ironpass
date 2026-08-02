"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { cn } from "@/lib/utils";
import {
  LayoutDashboard,
  ListVideo,
  Network,
  Globe,
  GitBranch,
  Fingerprint,
  Settings,
  Menu,
} from "lucide-react";
import {
  Sheet,
  SheetClose,
  SheetContent,
  SheetHeader,
  SheetTitle,
  SheetTrigger,
} from "@/components/ui/sheet";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";

const navItems = [
  { href: "/", label: "Dashboard", icon: LayoutDashboard },
  { href: "/subscriptions", label: "Subscriptions", icon: ListVideo },
  { href: "/nodes", label: "Nodes", icon: Network },
  { href: "/proxy", label: "Proxy", icon: Globe },
  { href: "/split-tunnel", label: "Split Tunnel", icon: GitBranch },
  { href: "/hwid", label: "HWID", icon: Fingerprint },
  { href: "/config", label: "Config", icon: Settings },
];

function NavLinks({ onClick }: { onClick?: () => void }) {
  const pathname = usePathname();
  return (
    <nav className="flex flex-col gap-1">
      {navItems.map((item) => {
        const active = pathname === item.href || pathname.startsWith(`${item.href}/`);
        return (
          <Link
            key={item.href}
            href={item.href}
            onClick={onClick}
            className={cn(
              "flex items-center gap-3 rounded-md px-3 py-2 text-sm font-medium transition-colors",
              active
                ? "bg-primary text-primary-foreground"
                : "hover:bg-muted text-foreground"
            )}
          >
            <item.icon className="size-4" />
            {item.label}
          </Link>
        );
      })}
    </nav>
  );
}

export function Navigation() {
  return (
    <>
      {/* Desktop sidebar */}
      <aside className="hidden md:flex w-60 flex-col border-r bg-card p-4">
        <div className="mb-6 px-2">
          <Link href="/" className="text-xl font-bold tracking-tight">
            IronPass
          </Link>
        </div>
        <NavLinks />
        <div className="mt-auto pt-4">
          <Separator />
          <p className="px-2 pt-4 text-xs text-muted-foreground">
            v{process.env.npm_package_version ?? "0.1.0"}
          </p>
        </div>
      </aside>

      {/* Mobile top bar */}
      <header className="md:hidden fixed top-0 left-0 right-0 z-40 flex h-14 items-center justify-between border-b bg-background px-4">
        <Link href="/" className="text-lg font-bold tracking-tight">
          IronPass
        </Link>
        <Sheet>
          <SheetTrigger asChild>
            <Button variant="ghost" size="icon" aria-label="Open menu">
              <Menu className="size-5" />
            </Button>
          </SheetTrigger>
          <SheetContent side="right" className="w-64">
            <SheetHeader>
              <SheetTitle>IronPass</SheetTitle>
            </SheetHeader>
            <div className="py-4">
              <SheetClose asChild>
                <div>
                  <NavLinks />
                </div>
              </SheetClose>
            </div>
          </SheetContent>
        </Sheet>
      </header>

      {/* Mobile spacer */}
      <div className="md:hidden h-14" />
    </>
  );
}
