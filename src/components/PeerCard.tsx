import React, { useState } from "react";
import {
  AlertTriangle,
  KeyRound,
  MonitorPlay,
  Play,
  Square,
  Wifi,
} from "lucide-react";
import type {
  HostReadiness,
  PeerInfo,
  RoutePath,
  SessionSnapshot,
  StreamPreset,
  StreamPresetId,
} from "../types";

interface PeerCardProps {
  peer: PeerInfo;
  presets: StreamPreset[];
  defaultPresetId: StreamPresetId;
  session: SessionSnapshot;
  onConnect: (
    peerId: string,
    presetId: StreamPresetId,
    appName: string | null,
    confirmIdentityChange?: boolean
  ) => Promise<void>;
  onDisconnect: (operationId?: string | null) => Promise<void>;
}

const READINESS_META: Record<
  HostReadiness,
  { label: string; color: string }
> = {
  inRoom: {
    label: "已进房",
    color: "bg-slate-700/70 text-slate-300 border-slate-600/50",
  },
  hostUnreachable: {
    label: "主机未开启被控",
    color: "bg-slate-800 text-slate-400 border-slate-700",
  },
  needsPairing: {
    label: "需要授权",
    color: "bg-amber-500/15 text-amber-300 border-amber-500/30",
  },
  streamReady: {
    label: "串流可用",
    color: "bg-emerald-500/15 text-emerald-300 border-emerald-500/30",
  },
  hostBusy: {
    label: "主机忙",
    color: "bg-rose-500/15 text-rose-300 border-rose-500/30",
  },
  identityChanged: {
    label: "身份已变更",
    color: "bg-rose-500/20 text-rose-300 border-rose-500/40",
  },
};

const PATH_META: Record<RoutePath, { label: string; color: string }> = {
  direct: { label: "直连 P2P", color: "text-emerald-400" },
  relay: { label: "中继转发", color: "text-amber-400" },
  unknown: { label: "路径未知", color: "text-slate-400" },
};

export const PeerCard: React.FC<PeerCardProps> = ({
  peer,
  presets,
  defaultPresetId,
  session,
  onConnect,
  onDisconnect,
}) => {
  const [presetId, setPresetId] = useState<StreamPresetId>(defaultPresetId);
  const [selectedApp, setSelectedApp] = useState<string>(
    peer.selectedApp ?? "Desktop"
  );

  const isThisPeerSession =
    session.peerId === peer.peerId &&
    !["idle", "failed", "disconnecting"].includes(session.stage);

  const readiness = READINESS_META[peer.readiness];
  const pathMeta = PATH_META[peer.path];

  const handleAction = async () => {
    if (isThisPeerSession) {
      await onDisconnect(session.operationId);
      return;
    }
    const confirmId = peer.readiness === "identityChanged";
    await onConnect(
      peer.peerId,
      presetId,
      peer.availableApps.length > 0 ? selectedApp : null,
      confirmId
    );
  };

  return (
    <div className="bg-surface-900 border border-slate-800 rounded-xl p-4 flex flex-col justify-between gap-4 hover:border-slate-700 transition-colors">
      <div className="space-y-3">
        <div className="flex items-start justify-between gap-2">
          <div className="min-w-0">
            <h3 className="text-base font-semibold text-slate-100 truncate">
              {peer.hostname}
            </h3>
            <p className="text-xs font-mono text-slate-400 mt-0.5">
              {peer.virtualIp}
            </p>
          </div>
          <span
            className={`text-[11px] px-2.5 py-1 rounded-full font-medium border shrink-0 ${readiness.color}`}
          >
            {readiness.label}
          </span>
        </div>

        {/* Three-tier connectivity indicators: 已进房 / 主机可达 / 串流可用 */}
        <div className="grid grid-cols-3 gap-2 text-[11px] bg-surface-950/60 rounded-lg p-2.5 border border-slate-800/70">
          <div>
            <div className="text-slate-500">路由路径</div>
            <div className={`font-medium mt-0.5 flex items-center gap-1 ${pathMeta.color}`}>
              <Wifi className="w-3 h-3" />
              {pathMeta.label}
            </div>
          </div>
          <div>
            <div className="text-slate-500">延迟 (RTT)</div>
            <div className="font-mono text-slate-200 mt-0.5">
              {peer.rttMs !== null ? `${peer.rttMs.toFixed(1)} ms` : "—"}
            </div>
          </div>
          <div>
            <div className="text-slate-500">主机服务</div>
            <div
              className={`font-medium mt-0.5 ${
                peer.hostReachable ? "text-emerald-400" : "text-slate-500"
              }`}
            >
              {peer.hostReachable ? "主机可达" : "未响应"}
            </div>
          </div>
        </div>

        {peer.readiness === "identityChanged" && (
          <div className="bg-rose-500/10 border border-rose-500/30 rounded-lg p-2.5 text-xs text-rose-200 flex items-start gap-2">
            <AlertTriangle className="w-4 h-4 text-rose-400 shrink-0 mt-0.5" />
            <span>
              该设备配对身份已变化，点击“确认新身份并连接”将重新核验授权。
            </span>
          </div>
        )}

        <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
          <div>
            <label className="block text-[11px] text-slate-400 mb-1">
              画质预设
            </label>
            <select
              value={presetId}
              disabled={isThisPeerSession}
              onChange={(e) => setPresetId(e.target.value as StreamPresetId)}
              className="w-full px-2.5 py-1.5 rounded-lg bg-surface-950 border border-slate-800 text-xs text-slate-200 focus:outline-none focus:border-sky-500 disabled:opacity-50"
            >
              {presets.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.label}
                </option>
              ))}
            </select>
          </div>

          <div>
            <label className="block text-[11px] text-slate-400 mb-1">
              远端应用
            </label>
            {peer.availableApps.length > 0 ? (
              <select
                value={selectedApp}
                disabled={isThisPeerSession}
                onChange={(e) => setSelectedApp(e.target.value)}
                className="w-full px-2.5 py-1.5 rounded-lg bg-surface-950 border border-slate-800 text-xs text-slate-200 focus:outline-none focus:border-sky-500 disabled:opacity-50"
              >
                {peer.availableApps.map((app) => (
                  <option key={app} value={app}>
                    {app}
                  </option>
                ))}
              </select>
            ) : (
              <div className="px-2.5 py-1.5 rounded-lg bg-surface-950/50 border border-slate-800/70 text-xs text-slate-500">
                Desktop (默认)
              </div>
            )}
          </div>
        </div>
      </div>

      <button
        type="button"
        disabled={
          (!isThisPeerSession && peer.readiness === "hostBusy") ||
          (!isThisPeerSession && !peer.hostReachable)
        }
        onClick={handleAction}
        className={`w-full py-2.5 px-4 rounded-lg text-sm font-medium inline-flex items-center justify-center gap-2 transition-colors disabled:opacity-40 disabled:cursor-not-allowed ${
          isThisPeerSession
            ? "bg-rose-600 hover:bg-rose-500 text-white"
            : peer.readiness === "needsPairing"
            ? "bg-amber-600 hover:bg-amber-500 text-white"
            : "bg-sky-600 hover:bg-sky-500 text-white"
        }`}
      >
        {isThisPeerSession ? (
          <>
            <Square className="w-4 h-4" />
            断开连接
          </>
        ) : peer.readiness === "needsPairing" ? (
          <>
            <KeyRound className="w-4 h-4" />
            首次授权并连接
          </>
        ) : peer.readiness === "identityChanged" ? (
          <>
            <MonitorPlay className="w-4 h-4" />
            确认新身份并连接
          </>
        ) : (
          <>
            <Play className="w-4 h-4" />
            一键连接桌面
          </>
        )}
      </button>
    </div>
  );
};
