-- Native compositor owns selection, focus and visibility. No actions or I/O.
local i,t=module.inputs,module.theme
local w,h=i.width or 900,i.height or 180
local p=widget.pixel_space(w,h)
widget.panel({id="ground",x=-4,y=4,width=8,height=8,ink="surface",opacity=1,radius=t.radius})
local count=0
for n=0,5 do if i["row"..n.."-title"] then count=count+1 end end
local gap,inset=10,18
local tile=(w-2*inset-gap*(math.max(1,count)-1))/math.max(1,count)
local function fit(text,size)
  local chars=math.max(1,math.floor((tile-24)/(size*0.65)))
  local cut=utf8.offset(text,chars+1)
  return cut and (string.sub(text,1,cut-1).."…") or text
end
for n=0,5 do
  local key="row"..n
  if i[key.."-title"] then
    local left=inset+n*(tile+gap)
    if i[key.."-selected"] then
      widget.panel({id=key.."-selection",x=p.x(left),y=p.y(18),width=p.w(tile),height=p.h(h-58),ink="primary",opacity=0.16,radius=t.radius})
      widget.panel({id=key.."-underline",x=p.x(left+12),y=p.y(h-48),width=p.w(tile-24),height=p.h(3),ink="primary",opacity=1,radius=1})
    end
    widget.label({id=key.."-app",text=fit(i[key.."-detail"] or "Application",t.small),x=p.x(left+12),y=p.y(34),size=t.small,ink="muted"})
    widget.label({id=key.."-title",text=fit(i[key.."-title"],t.body),x=p.x(left+12),y=p.y(62),size=t.body,ink="text"})
  end
end
widget.label({id="hint",text=i.health or "Alt + Tab",x=p.x(24),y=p.y(h-26),size=t.small,ink="muted"})
