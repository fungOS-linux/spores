-- Mock VFS search snapshot for ui/search.lua.
--
-- This is data, not presentation: the UI decides copy, density and layout.
-- Paths are workspace-relative; the Spore never carries a host path.

return {
  title = "Search",
  query = "greece",
  workspace = "vfs",
  mode = "substring",
  ready = true,
  generation = 3,
  total = 3,
  error = nil,
  selected_path = "photos/greece-01.jpg",
  filters = {
    { id = "all", label = "All files", count = 3, selected = true },
  },
  results = {
    {
      path = "notes/greece-notes.txt",
      name = "greece-notes.txt",
      media_type = "text/plain",
      size = 2048,
      kind = "file",
      is_dir = false,
      snippet = "trip notes for Athens and Nafplio",
      score = 12,
      matched_by = "text",
    },
    {
      path = "photos/greece-01.jpg",
      name = "greece-01.jpg",
      media_type = "image/jpeg",
      size = 512000,
      kind = "file",
      is_dir = false,
      snippet = nil,
      score = 9,
      matched_by = "path",
    },
    {
      path = "photos",
      name = "photos",
      media_type = "",
      size = 0,
      kind = "directory",
      is_dir = true,
      snippet = nil,
      score = 4,
      matched_by = "path",
    },
  },
  detail = {
    path = "photos/greece-01.jpg",
    name = "greece-01.jpg",
    media_type = "image/jpeg",
    size = 512000,
    kind = "file",
    is_dir = false,
    preview = "image/jpeg · 500 KB\n(binary preview omitted)",
  },
}
