import React from 'react';
import { ShieldCheck, ShieldAlert } from 'lucide-react';
import type { AuditReport, CategoryStat } from '../types/privacy';
import { formatFileSize } from '../lib/formatFileSize';
import { shouldShowZeroBytes } from '../lib/shouldShowZeroBytes';

interface AuditScreenProps {
  report: AuditReport;
}

function statFor(list: CategoryStat[], category: string): CategoryStat | undefined {
  return list.find((s) => s.category === category);
}

export const AuditScreen: React.FC<AuditScreenProps> = ({ report }) => {
  const categories = Array.from(
    new Set([...report.before.map((s) => s.category), ...report.after.map((s) => s.category)]),
  );
  const zero = shouldShowZeroBytes(report);

  return (
    <div className="fixed inset-0 z-[110] overflow-auto bg-surface-soft dark:bg-dark-canvas p-6">
      <div className="max-w-2xl mx-auto bg-white dark:bg-dark-card border border-hairline-soft dark:border-dark-hairline rounded-md shadow-airbnb p-8">
        <h1 className="text-2xl font-bold text-ink dark:text-dark-ink mb-4">Audit Burn Workspace</h1>

        {zero ? (
          <p className="flex items-center gap-2 text-airbnb-success font-semibold mb-4">
            <ShieldCheck size={20} aria-hidden="true" />
            <span>0 bytes remaining</span>
          </p>
        ) : (
          <div role="alert" className="flex items-start gap-2 text-airbnb-error mb-4">
            <ShieldAlert size={20} className="shrink-0 mt-0.5" aria-hidden="true" />
            <div className="text-sm">
              <p className="font-semibold">
                {report.verified
                  ? 'Verifikasi selesai, tetapi masih ada data tersisa.'
                  : 'Verifikasi tidak berhasil membuktikan Workspace kosong.'}
              </p>
              <p>Byte tersisa: {formatFileSize(report.remaining_bytes)} ({report.remaining_bytes} byte)</p>
              {report.incomplete && <p>Pembersihan atau verifikasi tidak lengkap.</p>}
              {report.remaining_categories.length > 0 && (
                <ul className="list-disc ml-5">
                  {report.remaining_categories.map((c) => (
                    <li key={c.category}>
                      {c.category}: {c.entries} entri, {c.bytes} byte
                    </li>
                  ))}
                </ul>
              )}
            </div>
          </div>
        )}

        <table className="w-full text-sm text-left mb-4 text-body dark:text-dark-body">
          <caption className="sr-only">Perbandingan Managed_Storage sebelum dan sesudah burn</caption>
          <thead>
            <tr className="border-b border-hairline-soft dark:border-dark-hairline text-ink dark:text-dark-ink">
              <th scope="col" className="py-2">Kategori</th>
              <th scope="col" className="py-2">Entri sebelum</th>
              <th scope="col" className="py-2">Byte sebelum</th>
              <th scope="col" className="py-2">Entri sesudah</th>
              <th scope="col" className="py-2">Byte sesudah</th>
            </tr>
          </thead>
          <tbody>
            {categories.map((cat) => {
              const b = statFor(report.before, cat);
              const a = statFor(report.after, cat);
              return (
                <tr key={cat} className="border-b border-hairline-soft dark:border-dark-hairline">
                  <th scope="row" className="py-2 font-medium">{cat}</th>
                  <td>{b?.entries ?? 0}</td>
                  <td>{b?.bytes ?? 0}</td>
                  <td>{a?.entries ?? 0}</td>
                  <td>{a?.bytes ?? 0}</td>
                </tr>
              );
            })}
          </tbody>
        </table>

        <dl className="grid grid-cols-2 gap-2 text-sm mb-4 text-body dark:text-dark-body">
          <dt className="font-medium text-ink dark:text-dark-ink">Durasi</dt>
          <dd>{report.duration_ms} ms</dd>
          <dt className="font-medium text-ink dark:text-dark-ink">Waktu (UTC)</dt>
          <dd>
            <time dateTime={report.timestamp_utc}>{report.timestamp_utc}</time>
          </dd>
          <dt className="font-medium text-ink dark:text-dark-ink">Item dihapus</dt>
          <dd>{report.removed_items}</dd>
        </dl>

        {report.failed_items.length > 0 && (
          <div className="text-sm text-airbnb-error mb-4">
            <p className="font-semibold">Item gagal dihapus:</p>
            <ul className="list-disc ml-5">
              {report.failed_items.map((f, i) => (
                <li key={`${f.category}-${i}`}>
                  {f.category}: {f.reason}
                </li>
              ))}
            </ul>
          </div>
        )}

        {report.warnings.length > 0 && (
          <div className="text-sm text-body dark:text-dark-body mb-4">
            <p className="font-semibold text-ink dark:text-dark-ink">Peringatan:</p>
            <ul className="list-disc ml-5">
              {report.warnings.map((w, i) => (
                <li key={i}>{w}</li>
              ))}
            </ul>
          </div>
        )}

        <p className="text-xs text-muted dark:text-dark-muted">
          Cakupan verifikasi adalah Managed_Storage (sesi, unduhan, sesi MCP, dan direktori
          sementara Workspace), bukan memori proses server secara keseluruhan.
        </p>
      </div>
    </div>
  );
};
