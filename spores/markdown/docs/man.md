# Reading

`markdown [FILE]` opens a document. A document is an opaque `DocId`, never a
path: the same content can live in the workspace, an attachment, or a synced
book, and the Spore sees one handle everywhere. The host resolves a path before
the Spore ever runs.

# Writing

The module owns text, cursor, undo and the parse. The view is Lua, and both hot
reload: changing `ui/editor.lua` restyles a running document without a rebuild
or a reload of the document itself.

Headings, paragraphs, quotes, ordered and unordered lists, rules and fenced code
are supported, as are inline strong, emphasis, code, links and images. Diagram
fences (`mermaid`, `pele`, `graphviz`, `plantuml`, `math`) are drawn by Canvas;
the Spore ships the source and never the rendered bytes.

# Storage

Documents are `DocId`s through the storage grant. Saving is debounced; a
conflict shows the stored version rather than silently overwriting it.

# What it cannot do

No network, no process, no secret, no clock, no filesystem path. Images and
attachments are `Target` values (`asset:`, `attach:`, `doc:`): the Spore names
them and Canvas fetches them.
