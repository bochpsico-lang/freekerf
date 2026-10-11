<script lang="ts">
  // A number field (design/design-system.md "Campos"): label, value, unit. Lengths
  // are kept in mm and shown in the user's units. Commits on Enter or blur only:
  // one action (and one undo step) per edit.
  let {
    label,
    value,
    unit = '',
    scale = 1,
    decimals = 2,
    min,
    max,
    title,
    testid,
    bare = false,
    oncommit,
  }: {
    label: string;
    /** In base units (mm, %, °...). */
    value: number;
    unit?: string;
    /** Shown = value / scale (25.4 for inches). */
    scale?: number;
    decimals?: number;
    min?: number;
    max?: number;
    title?: string;
    testid?: string;
    /** The label is shown elsewhere (above the field): only the accessible name. */
    bare?: boolean;
    oncommit: (value: number) => void;
  } = $props();

  const shown = $derived(Number((value / scale).toFixed(decimals)));

  function commit(e: Event & { currentTarget: HTMLInputElement }) {
    const v = Number(e.currentTarget.value);
    if (!Number.isFinite(v) || e.currentTarget.value.trim() === '') {
      e.currentTarget.value = String(shown);
      return;
    }
    let base = v * scale;
    if (min !== undefined) base = Math.max(min, base);
    if (max !== undefined) base = Math.min(max, base);
    if (Math.abs(base - value) > 1e-9) oncommit(base);
    else e.currentTarget.value = String(shown);
  }
</script>

<label class="num" {title}>
  {#if !bare}<span class="scrub">{label}</span>{/if}
  <input
    type="number"
    inputmode="decimal"
    step="any"
    value={shown}
    aria-label={label}
    data-testid={testid}
    onchange={commit}
    onkeydown={(e) => {
      if (e.key === 'Enter') e.currentTarget.blur();
    }}
  />
  {#if unit}<span class="unit">{unit}</span>{/if}
</label>
