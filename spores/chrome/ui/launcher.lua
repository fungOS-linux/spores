-- Logical pixels, shared desktop primitives, no dashboard chrome in Alt-Tab.
local i,t=module.inputs,module.theme
local w,h=i.width or 600,i.height or 440
local p=widget.pixel_space(w,h)
-- The sheet is behind everything it contains, so it says so: a ground that shares
-- a depth with the rows can end up in front of them, and an opaque ground in front
-- of the content is an empty launcher.
widget.panel({id="ground",x=-4,y=4,width=8,height=8,ink="surface",opacity=1,radius=t.radius,z=-1})
if i["window-cycle"] then
  local count=0
  for n=0,5 do if i["row"..n.."-title"] then count=count+1 end end
  count=math.max(1,count)
  local gap,inset=8,16
  local tile=(w-2*inset-gap*(count-1))/count
  for n=0,count-1 do
    local key="row"..n
    local title=i[key.."-title"]
    if title then
      local left=inset+n*(tile+gap)
      widget.icon_button({id=key,label=title,icon="window",icon_size=64,icon_caption=true,
        action="select",body={origin="desktop-launcher"},
        x=p.x(left),y=p.y(16),width=p.w(tile),height=p.h(h-32),
        enabled=i[key.."-available"]==true})
    end
  end
else
  local scope=i.scope or "all"
  local heading=scope=="workspaces" and "Workspaces" or scope=="native" and "Native apps" or scope=="canvas" and "Canvas apps" or "Applications"
  local hint=scope=="workspaces" and "Search workspaces" or scope=="native" and "Search native apps" or scope=="canvas" and "Search Canvas apps" or "Search apps, windows, workspaces"
  widget.label({id="heading",text=heading,x=p.x(24),y=p.y(22),size=t.title,ink="text"})
  widget.label({id="target",text=i.target or "This screen",x=p.x(w-144),y=p.y(28),size=t.small,ink="muted"})
  widget.panel({id="search-bg",x=p.x(20),y=p.y(62),width=p.w(w-40),height=p.h(38),ink="muted",opacity=0.08,radius=t.radius})
  widget.label({id="search",text=(i.query or "")=="" and hint or (i.query.."▏"),x=p.x(32),y=p.y(72),size=t.body,ink=(i.query or "")=="" and "muted" or "text"})
  -- Each row carries its own mark. The tint comes from the app's manifest, not
  -- from the shell's palette: a launcher that recolours apps makes two versions
  -- of one product look like different products.
  local function tint(value)
    if type(value) ~= "string" then return nil end
    return value:match("^#%x%x%x%x%x%x$") and value or nil
  end
  for n=0,5 do
    local key="row"..n
    local title=i[key.."-title"]
    if title then
      local top=112+n*44
      local fill=tint(i[key.."-tint"])
      -- A row without a manifest tint still gets a mark, so the column reads as a
      -- column of apps rather than as text that happens to start in two places.
      local mark_ink = fill and nil or "muted"
      if not fill then
        widget.panel({id=key.."-mark",x=p.x(24),y=p.y(top+6),width=p.w(26),height=p.h(26),
          ink="muted",opacity=0.16,radius=7,z=0.9})
      end
      if fill then
        widget.panel({id=key.."-mark",x=p.x(24),y=p.y(top+6),width=p.w(26),height=p.h(26),
          ink="surface",rgb=fill,opacity=0.92,radius=7,z=0.9})
      end
      -- Centred in a 26 pt tile: the label's y is its baseline, so it sits a few
      -- points below the tile's top rather than aligned to it.
      -- The label's y is where its box starts and text grows from there, so the
      -- monogram is placed at the tile's own top with a small optical inset.
      widget.label({id=key.."-glyph",text=i[key.."-badge"] or "•",
        x=p.x(24+(i[key.."-badge"] and 9 or 12)),y=p.y(top+4),
        size=t.small,ink=i[key.."-badge"] and "text" or "muted"})
      -- A launcher row is read left to right from its mark, so the label hugs the
      -- left edge instead of floating in the middle of the row.
      widget.button({id=key,label=title,appearance="quiet",align="left",
        action="select",body={origin="desktop-launcher"},
        x=p.x(56),y=p.y(top),width=p.w(w-76),height=p.h(40),
        enabled=i[key.."-available"]==true})
    end
  end
  if (i.count or 0)==0 then
    widget.label({id="empty",text=i.live and "No matches" or "Connecting…",x=p.x(32),y=p.y(132),size=t.body,ink="muted"})
  end
  if not i.live then
    widget.label({id="health",text=i.health or "Connecting…",x=p.x(24),y=p.y(h-30),size=t.small,ink="warning"})
  end
  if scope~="workspaces" then
    local other=scope=="native" and "canvas-apps" or "native-apps"
    widget.button({id=other,label=scope=="native" and "Canvas apps · F2" or "Native apps · F2",appearance="quiet",action="select",body={origin="desktop-launcher"},x=p.x(w-170),y=p.y(h-42),width=p.w(146),height=p.h(30),enabled=i.live==true})
  end
end
