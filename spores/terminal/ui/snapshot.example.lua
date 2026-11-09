-- Fixture for `tend check` and `tools/shot.lua`. The host owns the real frame;
-- this is the shape it hands the view, with the pieces a terminal draws: a
-- prompt, coloured output, an inverse selection and a cursor.
return {
  cols = 80,
  rows = 24,
  title = "avery@agora-one: ~",
  cursor = { x = 21, y = 3, visible = true },
  scroll = { offset = 0, total = 24, len = 24 },
  rows = {
    { y = 0, runs = { { "avery@agora-one ~ % ", "#7dd3a8", nil, 1 } } },
    { y = 1, runs = { { "ls -la", "#dce5ea", nil, 0 } } },
    { y = 2, runs = {
      { "total 24", "#8395a2", nil, 0 },
    } },
    { y = 3, runs = {
      { "drwxr-xr-x  4 avery staff  128 ", "#dce5ea", nil, 0 },
      { "src", "#7dd3a8", nil, 1 },
      { "  ", nil, nil, 0 },
      { "error: no such file", "#ff6b6b", "#2a1416", 0 },
    } },
    { y = 4, runs = { { "-rw-r--r--  1 avery staff 4096 README.md", "#dce5ea", nil, 0 } } },
    { y = 5, runs = { { "avery@agora-one ~ % ", "#7dd3a8", nil, 1 }, { "git status", "#dce5ea", nil, 0 } } },
    { y = 6, runs = { { "On branch main", "#dce5ea", nil, 0 } } },
    { y = 7, runs = { { "nothing to commit, working tree clean", "#8395a2", nil, 0 } } },
  },
}
