import React, { useState } from "react";
import {
  AlertTriangle,
  Check,
  Copy,
  Eye,
  EyeOff,
  LogOut,
  PlusCircle,
  RefreshCw,
  Wifi,
} from "lucide-react";
import type { AddressMode, RoomSnapshot } from "../types";

interface RoomBarProps {
  room: RoomSnapshot;
  defaultRelayNodes: string[];
  deviceName: string;
  onJoin: (
    roomName: string,
    roomSecret: string,
    addressMode: AddressMode,
    rememberSecret: boolean
  ) => Promise<void>;
  onLeave: () => Promise<void>;
  onGenerate: () => Promise<{ roomName: string; roomSecret: string }>;
  onLoadSaved: () => Promise<{ roomName: string; roomSecret: string } | null>;
  busy: boolean;
}

export const RoomBar: React.FC<RoomBarProps> = ({
  room,
  onJoin,
  onLeave,
  onGenerate,
  onLoadSaved,
  busy,
}) => {
  const [roomName, setRoomName] = useState(room.roomName ?? "");
  const [roomSecret, setRoomSecret] = useState("");
  const [showSecret, setShowSecret] = useState(false);
  const [rememberSecret, setRememberSecret] = useState(room.hasSavedSecret);
  const [useStatic, setUseStatic] = useState(false);
  const [staticCidr, setStaticCidr] = useState("10.144.144.2/24");
  const [copied, setCopied] = useState(false);

  const inRoom = room.stage === "inRoom" || room.stage === "joining";

  const handleGenerate = async () => {
    const creds = await onGenerate();
    setRoomName(creds.roomName);
    setRoomSecret(creds.roomSecret);
    setShowSecret(true);
  };

  const handleLoadSaved = async () => {
    const saved = await onLoadSaved();
    if (saved) {
      setRoomName(saved.roomName);
      setRoomSecret(saved.roomSecret);
      setRememberSecret(true);
    }
  };

  const handleCopyInvite = async () => {
    if (!room.roomName || !roomSecret) return;
    await navigator.clipboard.writeText(
      `EasyStream 房间名: ${room.roomName}\n房间密钥: ${roomSecret}`
    );
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    const mode: AddressMode = useStatic
      ? { static: { ipv4Cidr: staticCidr.trim() } }
      : "dhcp";
    await onJoin(roomName.trim(), roomSecret.trim(), mode, rememberSecret);
  };

  return (
    <div className="bg-surface-900 border border-slate-800 rounded-xl p-4 space-y-3">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2.5">
          <Wifi
            className={`w-5 h-5 ${
              inRoom ? "text-emerald-400" : "text-sky-400"
            }`}
          />
          <div>
            <h2 className="text-sm font-semibold text-slate-100">
              虚拟组网房间 (EasyTier v2.6.4)
            </h2>
            <p className="text-xs text-slate-400">
              两端使用相同的房间名与高熵密钥加入同一虚拟局域网，无需公网 IP 或路由器端口映射
            </p>
          </div>
        </div>

        {inRoom && (
          <div className="flex items-center gap-2">
            {roomSecret && (
              <button
                type="button"
                onClick={handleCopyInvite}
                className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium bg-surface-800 hover:bg-surface-700 text-slate-200 border border-slate-700 transition-colors"
              >
                {copied ? (
                  <Check className="w-3.5 h-3.5 text-emerald-400" />
                ) : (
                  <Copy className="w-3.5 h-3.5" />
                )}
                {copied ? "已复制房间凭据" : "复制房间口令"}
              </button>
            )}
            <button
              type="button"
              disabled={busy}
              onClick={onLeave}
              className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium bg-rose-500/15 hover:bg-rose-500/25 text-rose-300 border border-rose-500/30 transition-colors disabled:opacity-50"
            >
              <LogOut className="w-3.5 h-3.5" />
              离开房间
            </button>
          </div>
        )}
      </div>

      {!inRoom ? (
        <form onSubmit={handleSubmit} className="space-y-3">
          <div className="grid grid-cols-1 md:grid-cols-12 gap-3">
            <div className="md:col-span-4">
              <label className="block text-[11px] text-slate-400 mb-1">
                房间名称
              </label>
              <input
                type="text"
                value={roomName}
                onChange={(e) => setRoomName(e.target.value)}
                placeholder="例如: easystream-home"
                required
                className="w-full px-3 py-2 rounded-lg bg-surface-950 border border-slate-800 text-sm text-slate-100 placeholder:text-slate-600 focus:outline-none focus:border-sky-500"
              />
            </div>

            <div className="md:col-span-5">
              <label className="block text-[11px] text-slate-400 mb-1">
                房间密钥（至少 6 位，推荐随机高熵密钥）
              </label>
              <div className="relative">
                <input
                  type={showSecret ? "text" : "password"}
                  value={roomSecret}
                  onChange={(e) => setRoomSecret(e.target.value)}
                  placeholder="输入或点击生成高熵密钥"
                  required
                  minLength={6}
                  className="w-full px-3 py-2 pr-9 rounded-lg bg-surface-950 border border-slate-800 text-sm text-slate-100 placeholder:text-slate-600 focus:outline-none focus:border-sky-500 font-mono"
                />
                <button
                  type="button"
                  onClick={() => setShowSecret((v) => !v)}
                  className="absolute right-2.5 top-1/2 -translate-y-1/2 text-slate-400 hover:text-slate-200"
                >
                  {showSecret ? (
                    <EyeOff className="w-4 h-4" />
                  ) : (
                    <Eye className="w-4 h-4" />
                  )}
                </button>
              </div>
            </div>

            <div className="md:col-span-3 flex items-end gap-2">
              <button
                type="button"
                onClick={handleGenerate}
                disabled={busy}
                className="px-3 py-2 rounded-lg bg-surface-800 hover:bg-surface-700 text-slate-200 border border-slate-700 text-xs font-medium inline-flex items-center gap-1.5 transition-colors"
                title="创建新房间并生成随机高熵密钥"
              >
                <PlusCircle className="w-4 h-4 text-sky-400" />
                生成房间
              </button>
              <button
                type="submit"
                disabled={busy || !roomName.trim() || roomSecret.trim().length < 6}
                className="flex-1 px-4 py-2 rounded-lg bg-sky-600 hover:bg-sky-500 text-white text-sm font-medium transition-colors disabled:opacity-50"
              >
                {busy ? "入网中..." : "加入房间"}
              </button>
            </div>
          </div>

          <div className="flex flex-wrap items-center justify-between gap-3 text-xs text-slate-400">
            <div className="flex items-center gap-4">
              <label className="inline-flex items-center gap-1.5 cursor-pointer">
                <input
                  type="checkbox"
                  checked={rememberSecret}
                  onChange={(e) => setRememberSecret(e.target.checked)}
                  className="rounded border-slate-700 bg-surface-950 text-sky-500"
                />
                <span>保存密钥到 Windows 凭据管理器</span>
              </label>

              {room.hasSavedSecret && (
                <button
                  type="button"
                  onClick={handleLoadSaved}
                  className="inline-flex items-center gap-1 text-sky-400 hover:text-sky-300"
                >
                  <RefreshCw className="w-3 h-3" />
                  填充已保存房间
                </button>
              )}
            </div>

            <div className="flex items-center gap-3">
              <label className="inline-flex items-center gap-1.5 cursor-pointer">
                <input
                  type="checkbox"
                  checked={useStatic}
                  onChange={(e) => setUseStatic(e.target.checked)}
                  className="rounded border-slate-700 bg-surface-950 text-sky-500"
                />
                <span>使用静态虚拟 IP（网段冲突时启用）</span>
              </label>
              {useStatic && (
                <input
                  type="text"
                  value={staticCidr}
                  onChange={(e) => setStaticCidr(e.target.value)}
                  className="w-36 px-2 py-1 rounded bg-surface-950 border border-slate-700 text-xs font-mono text-slate-200"
                  placeholder="10.199.199.2/24"
                />
              )}
            </div>
          </div>
        </form>
      ) : (
        <div className="flex flex-wrap items-center gap-4 text-xs bg-surface-950/60 rounded-lg px-3.5 py-2.5 border border-slate-800/80">
          <div>
            <span className="text-slate-400">当前房间：</span>
            <span className="font-semibold text-slate-100 font-mono">
              {room.roomName}
            </span>
          </div>
          <div>
            <span className="text-slate-400">本机虚拟 IP：</span>
            <span className="font-semibold text-emerald-400 font-mono">
              {room.localVirtualIp ?? "DHCP 分配中..."}
            </span>
          </div>
          <div>
            <span className="text-slate-400">NAT 类型：</span>
            <span className="text-slate-200">{room.natType}</span>
          </div>
        </div>
      )}

      {room.subnetWarnings.length > 0 && (
        <div className="bg-amber-500/10 border border-amber-500/30 rounded-lg p-3 flex items-start gap-2.5 text-xs text-amber-200">
          <AlertTriangle className="w-4 h-4 text-amber-400 shrink-0 mt-0.5" />
          <div className="space-y-1">
            {room.subnetWarnings.map((w, i) => (
              <div key={i}>
                <span className="font-semibold">
                  网段重叠警告 ({w.localInterface}: {w.localCidr} ↔{" "}
                  {w.virtualCidr})：
                </span>
                {w.suggestion}
              </div>
            ))}
          </div>
        </div>
      )}

      {room.lastError && (
        <div className="bg-rose-500/10 border border-rose-500/30 rounded-lg p-3 text-xs text-rose-200">
          <span className="font-semibold">[{room.lastError.code}] </span>
          {room.lastError.message}
        </div>
      )}
    </div>
  );
};
