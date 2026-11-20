import React from "react";
import { RefreshCw, ShieldCheck, Trash2 } from "lucide-react";
import type { TrustedClient } from "../types";

interface TrustedClientsPanelProps {
  clients: TrustedClient[];
  hostEnabled: boolean;
  onRefresh: () => Promise<void>;
  onRevoke: (clientId: string) => Promise<void>;
  busy: boolean;
}

export const TrustedClientsPanel: React.FC<TrustedClientsPanelProps> = ({
  clients,
  hostEnabled,
  onRefresh,
  onRevoke,
  busy,
}) => {
  return (
    <div className="bg-surface-900 border border-slate-800 rounded-xl p-5 space-y-4">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2.5">
          <ShieldCheck className="w-5 h-5 text-emerald-400" />
          <div>
            <h2 className="text-base font-semibold text-slate-100">
              已授权控制端设备
            </h2>
            <p className="text-xs text-slate-400">
              以本机 Sunshine 持久配对库为准。撤销授权后将立即终止该设备的现有会话，旧证书不可再重连
            </p>
          </div>
        </div>

        <button
          type="button"
          disabled={busy || !hostEnabled}
          onClick={onRefresh}
          className="px-3 py-1.5 rounded-lg bg-surface-800 hover:bg-surface-700 text-slate-200 border border-slate-700 text-xs font-medium inline-flex items-center gap-1.5 disabled:opacity-40"
        >
          <RefreshCw className="w-3.5 h-3.5" />
          刷新列表
        </button>
      </div>

      {!hostEnabled ? (
        <div className="text-xs text-slate-500 py-8 text-center border border-dashed border-slate-800 rounded-lg">
          请先在“被控端与配对”中开启远程控制服务，以查看或撤销本机的已配对控制端。
        </div>
      ) : clients.length === 0 ? (
        <div className="text-xs text-slate-500 py-8 text-center border border-dashed border-slate-800 rounded-lg">
          当前没有已配对的控制端设备。
        </div>
      ) : (
        <div className="divide-y divide-slate-800/80 border border-slate-800 rounded-lg overflow-hidden">
          {clients.map((c) => (
            <div
              key={c.clientId}
              className="px-4 py-3 bg-surface-950/50 flex items-center justify-between gap-4"
            >
              <div>
                <div className="text-sm font-medium text-slate-100">
                  {c.name}
                </div>
                <div className="text-[11px] font-mono text-slate-500 mt-0.5">
                  UUID: {c.clientId}
                </div>
              </div>

              <button
                type="button"
                disabled={busy}
                onClick={() => onRevoke(c.clientId)}
                className="px-3 py-1.5 rounded-lg bg-rose-500/15 hover:bg-rose-500/25 text-rose-300 border border-rose-500/30 text-xs font-medium inline-flex items-center gap-1.5 transition-colors disabled:opacity-40"
              >
                <Trash2 className="w-3.5 h-3.5" />
                撤销授权
              </button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
};
