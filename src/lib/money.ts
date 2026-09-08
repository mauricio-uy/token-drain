const USD = new Intl.NumberFormat("en-US", { style: "currency", currency: "USD" });
const COMPACT_USD = new Intl.NumberFormat("en-US", {
  style: "currency", currency: "USD", notation: "compact", maximumFractionDigits: 1,
});

export function formatUsd(amount: number | null | undefined, compact = false): string {
  if (amount === null || amount === undefined || !Number.isFinite(amount)) return "—";
  return (compact && Math.abs(amount) >= 1000 ? COMPACT_USD : USD).format(amount);
}
