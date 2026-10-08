// Feature flags, fixed at build time.
//
// `designMode`: the Design mode screens are built against the UI contract and the
// fake service until M2 lands (M4b); distributed builds keep them off. Turn on with
// `VITE_FK_DESIGN_MODE=1` (dev server and tests have it on).

export const flags = {
  designMode: import.meta.env.DEV || import.meta.env.VITE_FK_DESIGN_MODE === '1',
} as const;
