-- Two products, five segments. Annual revenue per account, then per 100
-- customers. Figures are the ones `tests/pricing_strategy.rs` computes:
-- SaaS $120-150/month including 2,000 parts, $0.10/part overage, SDK bundle $50k.
return {
  tab = "pricing",
  title = "Pricing",
  subtitle = "Two products · annual revenue per account",
  provenance = "planning estimate",
  column_label = "SEGMENT",
  label_width = 300,
  columns = { "Annual", "Monthly", "Revenue share" },
  column_formats = { "money", "money", "percent" },
  rows = {
    { id = "s-small", label = "Small shop · SaaS", format = "money",
      values = { 1440, 120, 16.8 } },
    { id = "s-growing", label = "Growing shop · SaaS", format = "money",
      values = { 1800, 150, 8.7 } },
    { id = "s-bureau", label = "Bureau · SaaS + overage", format = "money",
      values = { 11400, 950, 22.1 } },
    { id = "s-platform", label = "Platform ISV · SDK", format = "money",
      values = { 50000, 4167, 38.8 } },
    { id = "s-giant", label = "Giant platform · SDK on-prem", format = "money",
      values = { 70000, 5833, 13.6 } },
    { id = "s-total", label = "Total per 100 customers", format = "money", ink = "primary",
      values = { 515400, 42950, 100.0 } },
  },
  assumptions = {
    { label = "SaaS · includes 2,000 parts", value = "$99 / $125 / $149; selected $125" },
    { label = "Overage above the allowance", value = "$0.10 / part" },
    { label = "SDK + API · bundle · on-prem", value = "$60k · $50k · $70k" },
    { label = "SaaS-to-licence break-even", value = "42,417 parts / month" },
    { label = "Licence only when", value = "metered bill exceeds bundle" },
    { label = "Support rate", value = "18% of licence" },
  },
}
