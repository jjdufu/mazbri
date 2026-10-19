export type HoppAutoCompleteOption = string | { label: string; value: string };

export function hoppAutoCompleteLabel(opt: HoppAutoCompleteOption): string {
  return typeof opt === "string" ? opt : opt.label;
}

export function hoppAutoCompleteValue(opt: HoppAutoCompleteOption): string {
  return typeof opt === "string" ? opt : opt.value;
}


export function filterHoppAutoCompleteOptions(
  options: readonly HoppAutoCompleteOption[],
  query: string,
): HoppAutoCompleteOption[] {
  const q = query.trim().toLowerCase();
  if (!q) return [...options];
  return options.filter((opt) =>
    hoppAutoCompleteLabel(opt).toLowerCase().includes(q),
  );
}
