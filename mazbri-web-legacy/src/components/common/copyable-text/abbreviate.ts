/** 长标识缩写：超过 16 个字符时显示前 8 位…后 6 位。 */
export function abbreviateId(value: string): string {
  const trimmed = value.trim();
  if (trimmed.length <= 16) return trimmed;
  return `${trimmed.slice(0, 8)}…${trimmed.slice(-6)}`;
}
