-- Mock contacts snapshot for ui/contacts.lua. Synthetic data only.
-- Generate a real one with tools/records_contacts_snapshot.py against a records
-- book, or pass a path as the third argument to tools/compose_inbox.lua.

return {
  account = { label = "Records", sync = "synced" },
  total = 6,
  selected_id = "c-ada",
  contacts = {
    { id = "c-ada", name = "Ada Lovelace", org = "Analytical Engines", title = "Engineer" },
    { id = "c-grace", name = "Grace Hopper", org = "Navy Computing", title = "Rear Admiral" },
    { id = "c-alan", name = "Alan Turing", org = "Bletchley Park", title = "Cryptanalyst" },
    { id = "c-katherine", name = "Katherine Johnson", org = "NASA", title = "Mathematician" },
    { id = "c-edsger", name = "Edsger Dijkstra", org = "TU Eindhoven", title = "Computer Scientist" },
    { id = "c-margaret", name = "Margaret Hamilton", org = "MIT Instrumentation", title = "Software Engineer" },
  },
}
