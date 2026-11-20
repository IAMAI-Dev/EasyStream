import React from "react";
import {
  Activity,
  Monitor,
  Radio,
  Settings,
  ShieldCheck,
  Users,
} from "lucide-react";
import type { HostStatus, RoomSnapshot } from "../types";

export type NavTab = "lobby" | "host" | "trusted" | "diagnostics" | "settings";

interface SidebarProps {
  activeTab: NavTab;
  onSelectTab: (tab: NavTab) => void;
  room: RoomSnapshot;
  host: HostStatus;
  peerCount: number;
}

export const Sidebar: React.FC<SidebarProps> = ({
  activeTab,
  onSelectTab,
  room,
  host,
  peerCount,
}) => {
  const navItems: {
    id: NavTab;
    label: string;
    icon: React.ReactNode;
    badge?: string | number;
    badgeColor?: string;
  }[] = [
    {
      id: "lobby",
      label: "设备大厅",
      icon: <Users className="w-4 h-4" />,
      badge: peerCount > 0 ? peerCount : undefined,
      badgeColor: "bg-sky-500/20 text-sky-300",
    },
    {
      id: "host",
      label: "被控端与配对",
      icon: <Monitor className="w-4 h-4" />,
      badge:
        host.pendingPairings.length > 0
          ? `${host.pendingPairings.length} 待授权`
          : host.enabled
          ? "被控中"
          : undefined,
      badgeColor:
        host.pendingPairings.length > 0
          ? "bg-amber-500/20 text-amber-300"
          : "bg-emerald-500/20 text-emerald-300",
    },
    {
      id: "trusted",
      label: "已授权设备",
      icon: <ShieldCheck className="w-4 h-4" />,
      badge:
        host.trustedClients.length > 0 ? host.trustedClients.length : undefined,
      badgeColor: "bg-slate-700 text-slate-300",
    },
    {
      id: "diagnostics",
      label: "诊断与组件",
      icon: <Activity className="w-4 h-4" />,
    },
    {
      id: "settings",
      label: "偏好设置",
      icon: <Settings className="w-4 h-4" />,
    },
  ];

  const inRoom = room.stage === "inRoom";

  return (
    <aside className="w-60 shrink-0 bg-surface-900 border-r border-slate-800/80 flex flex-col justify-between select-none">
      <div>
        <div className="px-5 py-5 flex items-center gap-3 border-b border-slate-800/70">
          <div className="w-9 h-9 rounded-xl bg-gradient-to-br from-sky-500 to-indigo-600 flex items-center justify-center shadow-lg shadow-sky-900/40">
            <Radio className="w-5 h-5 text-white" />
          </div>
          <div>
            <h1 className="text-base font-bold tracking-tight text-slate-100">
              EasyStream
            </h1>
            <p className="text-[11px] text-slate-400">跨网桌面与游戏串流</p>
          </div>
        </div>

        <nav className="p-3 space-y-1">
          {navItems.map((item) => {
            const active = activeTab === item.id;
            return (
              <button
                key={item.id}
                onClick={() => onSelectTab(item.id)}
                className={`w-full flex items-center justify-between px-3 py-2.5 rounded-lg text-sm font-medium transition-colors ${
                  active
                    ? "bg-sky-500/15 text-sky-300 border border-sky-500/30"
                    : "text-slate-400 hover:text-slate-200 hover:bg-surface-800/70 border border-transparent"
                }`}
              >
                <span className="flex items-center gap-2.5">
                  {item.icon}
                  {item.label}
                </span>
                {item.badge !== undefined && (
                  <span
                    className={`text-[11px] px-2 py-0.5 rounded-full font-semibold ${item.badgeColor}`}
                  >
                    {item.badge}
                  </span>
                )}
              </button>
            );
          })}
        </nav>
      </div>

      <div className="p-4 border-t border-slate-800/70 space-y-2.5 text-xs">
        <div className="bg-surface-800/80 rounded-lg p-3 border border-slate-800">
          <div className="flex items-center justify-between mb-1.5">
            <span className="text-slate-400">本机虚拟节点</span>
            <span
              className={`inline-flex items-center gap-1.5 font-medium ${
                inRoom ? "text-emerald-400" : "text-slate-500"
              }`}
            >
              <span
                className={`w-2 h-2 rounded-full ${
                  inRoom ? "bg-emerald-400 animate-pulse" : "bg-slate-600"
                }`}
              />
              {inRoom ? "已进房" : "未入网"}
            </span>
          </div>
          <div className="font-mono text-slate-200 truncate">
            {room.localVirtualIp ?? "—"}
          </div>
          <div className="text-[11px] text-slate-500 mt-1 flex justify-between">
            <span className="truncate">{room.localHostname}</span>
            <span>{room.natType}</span>
          </div>
        </div>
        <div className="text-[11px] text-slate-500 text-center">
          GPLv3 · EasyTier + Sunshine + Moonlight
        </div>
      </div>
    </aside>
  );
};
