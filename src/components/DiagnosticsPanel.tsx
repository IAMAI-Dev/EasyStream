import React, { useState } from "react";
import {
  CheckCircle2,
  FileDown,
  Network,
  PackageCheck,
  Terminal,
  XCircle,
} from "lucide-react";
import type { ComponentStatusItem, RouteDiagnosticEntry } from "../types";

interface DiagnosticsPanelProps {
  components: ComponentStatusItem[];
  routes: RouteDiagnosticEntry[];
  logs: string[];
  onRefreshLogs: () => Promise<void>;
  onExportDiagnostics: () => Promise<string>;
}

export const DiagnosticsPanel: React.FC<DiagnosticsPanelProps> = ({
  components,
  routes,
  logs,
  onRefreshLogs,
  onExportDiagnostics,
}) => {
  const [exportPath, setExportPath] = useState<string | null>(null);
  const [exporting, setExporting] = useState(false);

  const handleExport = async () => {
    setExporting(true);
    try {
      const path = await onExportDiagnostics();
      setExportPath(path);
    } finally {
      setExporting(false);
    }
  };

  return (
    <div className="space-y-4">
      {/* Pinned Component Matrix */}
      <div className="bg-surface-900 border border-slate-800 rounded-xl p-5 space-y-3">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <PackageCheck className="w-5 h-5 text-sky-400" />
            <div>
              <h2 className="text-base font-semibold text-slate-100">
                上游组件基线与许可证清单
              </h2>
              <p className="text-xs text-slate-400">
                EasyStream 本身采用 GPLv3 协议，按版本锁定并校验外部组件
              </p>
            </div>
          </div>

          <button
            type="button"
            disabled={exporting}
            onClick={handleExport}
            className="px-3.5 py-2 rounded-lg bg-sky-600 hover:bg-sky-500 text-white text-xs font-medium inline-flex items-center gap-1.5 transition-colors disabled:opacity-50"
          >
            <FileDown className="w-4 h-4" />
            {exporting ? "导出中..." : "导出脱敏诊断包"}
          </button>
        </div>

        {exportPath && (
          <div className="bg-emerald-500/10 border border-emerald-500/30 rounded-lg p-3 text-xs text-emerald-200 font-mono break-all">
            已导出脱敏诊断报告（已排除房间密钥、管理密码、PIN 与私钥）：{exportPath}
          </div>
        )}

        <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
          {components.map((comp) => (
            <div
              key={comp.id}
              className="bg-surface-950/60 border border-slate-800 rounded-lg p-3.5 space-y-1.5"
            >
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-2">
                  {comp.found ? (
                    <CheckCircle2 className="w-4 h-4 text-emerald-400" />
                  ) : (
                    <XCircle className="w-4 h-4 text-rose-400" />
                  )}
                  <span className="text-sm font-semibold text-slate-100">
                    {comp.name}
                  </span>
                </div>
                <span className="text-[11px] font-mono px-2 py-0.5 rounded bg-slate-800 text-slate-300">
                  {comp.pinnedVersion} · {comp.license}
                </span>
              </div>
              <div className="text-xs text-slate-400">{comp.note}</div>
              <div className="text-[11px] font-mono text-slate-500 truncate">
                {comp.path ?? "未就绪 — 请运行 scripts/fetch-upstream.ps1"}
              </div>
            </div>
          ))}
        </div>
      </div>

      {/* Route table */}
      <div className="bg-surface-900 border border-slate-800 rounded-xl p-5 space-y-3">
        <div className="flex items-center gap-2">
          <Network className="w-4 h-4 text-sky-400" />
          <h3 className="text-sm font-semibold text-slate-100">
            EasyTier 路由快照
          </h3>
        </div>

        {routes.length === 0 ? (
          <div className="text-xs text-slate-500 py-4 text-center border border-dashed border-slate-800 rounded-lg">
            加入房间后将在此显示节点路由与下一跳路径。
          </div>
        ) : (
          <div className="overflow-x-auto border border-slate-800 rounded-lg">
            <table className="w-full text-left text-xs">
              <thead className="bg-surface-950 text-slate-400 border-b border-slate-800">
                <tr>
                  <th className="px-3 py-2">主机名</th>
                  <th className="px-3 py-2">虚拟 IP</th>
                  <th className="px-3 py-2">下一跳</th>
                  <th className="px-3 py-2">跳数</th>
                  <th className="px-3 py-2">路径延迟</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-800/70 font-mono">
                {routes.map((r, i) => (
                  <tr key={i} className="hover:bg-surface-800/40">
                    <td className="px-3 py-2 text-slate-200 font-sans">
                      {r.hostname}
                    </td>
                    <td className="px-3 py-2 text-slate-300">{r.ipv4}</td>
                    <td className="px-3 py-2 text-slate-300">
                      {r.nextHopIpv4 || "—"} ({r.nextHopHostname || "—"})
                    </td>
                    <td className="px-3 py-2 text-slate-300">{r.pathLen}</td>
                    <td className="px-3 py-2 text-slate-300">
                      {r.pathLatencyMs > 0 ? `${r.pathLatencyMs} ms` : "—"}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {/* Redacted live logs */}
      <div className="bg-surface-900 border border-slate-800 rounded-xl p-5 space-y-3">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <Terminal className="w-4 h-4 text-slate-400" />
            <h3 className="text-sm font-semibold text-slate-100">
              脱敏运行日志
            </h3>
          </div>
          <button
            type="button"
            onClick={onRefreshLogs}
            className="text-xs text-sky-400 hover:text-sky-300"
          >
            刷新日志
          </button>
        </div>

        <div className="bg-surface-950 border border-slate-800 rounded-lg p-3 h-44 overflow-y-auto font-mono text-[11px] text-slate-300 space-y-1">
          {logs.length === 0 ? (
            <div className="text-slate-600">暂无运行日志。</div>
          ) : (
            logs.map((l, idx) => <div key={idx}>{l}</div>)
          )}
        </div>
      </div>
    </div>
  );
};
