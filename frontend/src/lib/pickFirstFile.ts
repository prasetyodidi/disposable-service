export interface PickFirstFileResult<T> {
  file: T | null;
  /** true jika dan hanya jika lebih dari satu file diberikan. */
  multiple: boolean;
}

/** Ambil hanya file pertama; tandai pemberitahuan bila n > 1. */
export function pickFirstFile<T>(files: ArrayLike<T> | null | undefined): PickFirstFileResult<T> {
  const n = files?.length ?? 0;
  if (!files || n === 0) return { file: null, multiple: false };
  return { file: files[0], multiple: n > 1 };
}
