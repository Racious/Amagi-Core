import type { Metadata } from "next";
import { headers } from "next/headers";
import "@fontsource-variable/noto-sans-tc/wght.css";
import "@fontsource-variable/noto-serif-tc/wght.css";
import "@fontsource-variable/space-grotesk/wght.css";
import "./globals.css";

export async function generateMetadata(): Promise<Metadata> {
  const requestHeaders = await headers();
  const host = requestHeaders.get("x-forwarded-host") ?? requestHeaders.get("host") ?? "localhost:3000";
  const protocol = requestHeaders.get("x-forwarded-proto") ?? (host.startsWith("localhost") ? "http" : "https");
  const origin = `${protocol}://${host}`;

  return {
    metadataBase: new URL(origin),
    title: "AMAGI Core｜AI 記憶與技能同步管家",
    description: "把開發變更轉成可審核、可同步、可跨機延續的 AI 專案記憶與技能。支援 Claude Code 與 Codex。",
    icons: {
      icon: "/favicon.png",
      shortcut: "/favicon.png",
    },
    openGraph: {
      type: "website",
      locale: "zh_TW",
      url: "/",
      siteName: "AMAGI Core",
      title: "AMAGI Core｜讓 AI 真正記得你的專案",
      description: "可審核、可同步、可跨機延續的 AI 專案記憶與技能中樞。",
      images: [
        {
          url: "/og.png",
          width: 1536,
          height: 1024,
          alt: "AMAGI Core AI 記憶與技能同步管家",
        },
      ],
    },
    twitter: {
      card: "summary_large_image",
      title: "AMAGI Core｜讓 AI 真正記得你的專案",
      description: "可審核、可同步、可跨機延續的 AI 專案記憶與技能中樞。",
      images: ["/og.png"],
    },
  };
}

export default function RootLayout({ children }: Readonly<{ children: React.ReactNode }>) {
  return (
    <html lang="zh-Hant">
      <body>{children}</body>
    </html>
  );
}
