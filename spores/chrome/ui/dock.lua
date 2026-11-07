-- Open-window indicators are state, not fabricated stream-health lights.
local i,t=module.inputs,module.theme
local w,h=math.max(i.width,1),math.max(i.height,1)
local p=widget.pixel_space(w,h)
local tile,gap,inset=52,8,12
local visible=math.max(0,math.min(6,math.floor((w-2*inset-72)/60),i.count))
local total=2*inset+tile+(visible>0 and 16+visible*(tile+gap)-gap or 0)
local left=(w-total)/2
widget.panel({id="ground",x=p.x(left),y=p.y(2),width=p.w(total),height=p.h(h-4),ink="surface",opacity=1,radius=t.radius})
widget.icon_button({id="launcher",label="Canvas apps",icon="apps",action="launcher",body={origin="desktop-dock"},x=p.x(left+inset),y=p.y(8),width=p.w(tile),height=p.h(48)})
if visible>0 then
  widget.panel({id="separator",x=p.x(left+inset+tile+8),y=p.y(20),width=p.w(1),height=p.h(28),ink="muted",opacity=0.18,radius=0})
end
for n=0,visible-1 do
  local key="item"..n
  local px=left+inset+tile+16+n*(tile+gap)
  local focused=i[key.."-focused"]==true
  widget.icon_button({id=key,label=i[key.."-title"],icon="window",icon_size=32,action="focus",body={surface_id=i[key.."-id"]},x=p.x(px),y=p.y(8),width=p.w(tile),height=p.h(48),enabled=i[key.."-available"]~=false})
  if i[key.."-open"]~=false then
    widget.panel({id=key.."-state",x=p.x(px+(tile-(focused and 16 or 4))/2),y=p.y(61),width=p.w(focused and 16 or 4),height=p.h(3),ink=focused and "primary" or "muted",opacity=i[key.."-minimized"] and 0.35 or 1,radius=2})
  end
end
