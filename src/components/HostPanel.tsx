import React, { useState } from "react";
import {
  AlertCircle,
  CheckCircle2,
  Download,
  KeyRound,
  Monitor,
  Power,
  Shield,
  XCircle,
} from "lucide-react";
import type { DecidePairingRequest, HostStatus } from "../types";

interface HostPanelProps {
  host: HostStatus;
  onToggleHost: (enable: boolean) => Promise<void>;
  onInstallSunshine: () => Promise<void>;
  onDecidePairing: (req: DecidePairingRequest) => Promise<void>;
  busy: boolean;
}

export const HostPanel: React.FC<HostPanelProps> = ({
  host,
  onToggleHost,
  onInstallSunshine,
  onDecidePairing,
  busy,
}) => {
  const [pinInputs, setPinInputs] = useState<Record<string, string>>({});
  const [nameInputs, setNameInputs] = useState<Record<string, string>>({});

  const handlePinChange = (id: string, val: string) => {
    const digits = val.replace(/\D/g, "").slice(0, 4);
    setPinInputs((prev) => ({ ...prev, [id]: digits }));
  };

  return (
    <div className="space-y-4">
      {/* Main host toggle card */}
      <div className="bg-surface-900 border border-slate-800 rounded-xl p-5 space-y-4">
        <div className="flex flex-wrap items-center justify-between gap-4">
          <div className="flex items-center gap-3">
            <div
              className={`w-11 h-11 rounded-xl flex items-center justify-center ${
                host.enabled
                  ? "bg-emerald-500/20 text-emerald-400 border border-emerald-500/30"
                  : "bg-surface-800 text-slate-400 border border-slate-700"
              }`}
            >
              <Monitor className="w-6 h-6" />
            </div>
            <div>
              <h2 className="text-base font-semibold text-slate-100">
                允许远程控制本机（被控端）
              </h2>
              <p className="text-xs text-slate-400 mt-0.5">
                默认关闭。开启后将启动受管 Sunshine 服务、配置回环管理凭据并仅向 EasyTier 虚拟网卡放行媒体端口
              </p>
            </div>
          </div>

          <div className="flex items-center gap-2.5">
            {!host.sunshineInstalled ? (
              <button
                type="button"
                disabled={busy}
                onClick={onInstallSunshine}
                className="px-4 py-2 rounded-lg bg-sky-600 hover:bg-sky-500 text-white text-sm font-medium inline-flex items-center gap-2 transition-colors disabled:opacity-50"
              >
                <Download className="w-4 h-4" />
                安装 Sunshine 组件
              </button>
            ) : (
              <button
                type="button"
                disabled={busy || host.sunshineConflictDetected}
                onClick={() => onToggleHost(!host.enabled)}
                className={`px-4 py-2 rounded-lg text-sm font-medium inline-flex items-center gap-2 transition-colors disabled:opacity-40 ${
                  host.enabled
                    ? "bg-rose-600 hover:bg-rose-500 text-white"
                    : "bg-emerald-600 hover:bg-emerald-500 text-white"
                }`}
              >
                <Power className="w-4 h-4" />
                {host.enabled ? "立即停止被控" : "允许远程控制"}
              </button>
            )}
          </div>
        </div>

        {host.sunshineConflictDetected && host.sunshineConflictMessage && (
          <div className="bg-amber-500/10 border border-amber-500/30 rounded-lg p-3.5 flex items-start gap-2.5 text-xs text-amber-200">
            <AlertCircle className="w-4 h-4 text-amber-400 shrink-0 mt-0.5" />
            <div>
              <div className="font-semibold mb-0.5">检测到独立 Sunshine 安装</div>
              <div>{host.sunshineConflictMessage}</div>
            </div>
          </div>
        )}

        {host.lastError && (
          <div className="bg-rose-500/10 border border-rose-500/30 rounded-lg p-3 text-xs text-rose-200">
            <span className="font-semibold">[{host.lastError.code}] </span>
            {host.lastError.message}
          </div>
        )}

        {/* Security & Display Summary */}
        <div className="grid grid-cols-1 md:grid-cols-3 gap-3 text-xs">
          <div className="bg-surface-950/60 border border-slate-800/80 rounded-lg p-3">
            <div className="text-slate-400 mb-1">Sunshine 服务状态</div>
            <div className="font-medium text-slate-100 flex items-center gap-1.5">
              <span
                className={`w-2 h-2 rounded-full ${
                  host.serviceRunning ? "bg-emerald-400" : "bg-slate-600"
                }`}
              />
              {host.serviceRunning ? "运行中" : "已停止"}
              {host.sunshineVersion ? ` (${host.sunshineVersion})` : ""}
            </div>
          </div>

          <div className="bg-surface-950/60 border border-slate-800/80 rounded-lg p-3">
            <div className="text-slate-400 mb-1 flex items-center gap-1">
              <Shield className="w-3.5 h-3.5 text-sky-400" />
              网络边界与防火墙
            </div>
            <div className="text-slate-200">
              {host.firewallScoped
                ? `已限定虚拟网卡 · 管理端口 ${host.managementPort} 仅回环`
                : `管理端口 ${host.managementPort} 仅本机回环`}
            </div>
          </div>

          <div className="bg-surface-950/60 border border-slate-800/80 rounded-lg p-3">
            <div className="text-slate-400 mb-1">可捕获显示输出</div>
            <div className="text-slate-200">
              {host.captureDisplays.length > 0
                ? `${host.captureDisplays.length} 个活动输出 (${host.captureDisplays[0].width}x${host.captureDisplays[0].height}@${host.captureDisplays[0].refreshHz}Hz)`
                : "未检测到活动输出"}
            </div>
          </div>
        </div>

        <div className="text-xs text-slate-400 bg-surface-950/40 rounded-lg px-3 py-2 border border-slate-800/60">
          {host.virtualDisplay.guidance}
        </div>
      </div>

      {/* Pending pairing approvals */}
      <div className="bg-surface-900 border border-slate-800 rounded-xl p-5 space-y-3">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <KeyRound className="w-4 h-4 text-amber-400" />
            <h3 className="text-sm font-semibold text-slate-100">
              待审批的连接配对请求
            </h3>
          </div>
          <span className="text-xs text-slate-400">
            {host.pendingPairings.length} 个请求
          </span>
        </div>

        {host.pendingPairings.length === 0 ? (
          <div className="text-xs text-slate-500 py-6 text-center border border-dashed border-slate-800 rounded-lg">
            暂无待配对请求。当控制端首次点击连接时，此处会弹出该设备的配对条目，请输入控制端显示的 4 位短码完成授权。
          </div>
        ) : (
          <div className="space-y-3">
            {host.pendingPairings.map((req) => {
              const pinVal = pinInputs[req.requestId] ?? "";
              const nameVal = nameInputs[req.requestId] ?? req.clientName;
              return (
                <div
                  key={req.requestId}
                  className="bg-surface-950/70 border border-amber-500/30 rounded-lg p-4 flex flex-col md:flex-row md:items-center justify-between gap-4"
                >
                  <div className="space-y-1">
                    <div className="flex items-center gap-2">
                      <span className="text-sm font-semibold text-slate-100">
                        {req.clientName}
                      </span>
                      <span className="text-[11px] font-mono px-2 py-0.5 rounded bg-slate-800 text-slate-300">
                        来源: {req.sourceAddress || "虚拟网络"}
                      </span>
                    </div>
                    <div className="text-[11px] font-mono text-slate-500">
                      请求 ID: {req.pairingId}
                    </div>
                  </div>

                  <div className="flex flex-wrap items-center gap-2">
                    <input
                      type="text"
                      value={nameVal}
                      onChange={(e) =>
                        setNameInputs((prev) => ({
                          ...prev,
                          [req.requestId]: e.target.value,
                        }))
                      }
                      placeholder="设备备注名"
                      className="w-32 px-2.5 py-1.5 rounded-lg bg-surface-900 border border-slate-700 text-xs text-slate-100"
                    />
                    <input
                      type="text"
                      inputMode="numeric"
                      maxLength={4}
                      value={pinVal}
                      onChange={(e) =>
                        handlePinChange(req.requestId, e.target.value)
                      }
                      placeholder="4位短码"
                      className="w-24 px-2.5 py-1.5 rounded-lg bg-surface-900 border border-amber-500/50 text-sm font-mono tracking-widest text-center text-amber-200 focus:outline-none focus:border-amber-400"
                    />
                    <button
                      type="button"
                      disabled={busy || pinVal.length !== 4}
                      onClick={() =>
                        onDecidePairing({
                          requestId: req.requestId,
                          approve: true,
                          pin: pinVal,
                          clientName: nameVal,
                        })
                      }
                      className="px-3 py-1.5 rounded-lg bg-emerald-600 hover:bg-emerald-500 text-white text-xs font-medium inline-flex items-center gap-1 disabled:opacity-40"
                    >
                      <CheckCircle2 className="w-3.5 h-3.5" />
                      确认授权
                    </button>
                    <button
                      type="button"
                      disabled={busy}
                      onClick={() =>
                        onDecidePairing({
                          requestId: req.requestId,
                          approve: false,
                        })
                      }
                      className="px-3 py-1.5 rounded-lg bg-rose-500/15 hover:bg-rose-500/25 text-rose-300 border border-rose-500/30 text-xs font-medium inline-flex items-center gap-1"
                    >
                      <XCircle className="w-3.5 h-3.5" />
                      拒绝
                    </button>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
};
