-- usage: lua bake_animation.lua <transform|seek|ae|distort> <file> <samples> <duration>
local mode, path, n, duration = arg[1], arg[2], tonumber(arg[3]), tonumber(arg[4])
math.pow = math.pow or function(a,b) return a^b end
cjson = {new = function() return {} end}
local function vector(...) return {...} end
local v3 = setmetatable({Distance = function() return 0 end}, {__call = function(_, ...) return vector(...) end})
Amaz = {Vector2f = vector, Vector3f = v3, Color = vector, Ease = {
    linearFunc = function(t,c,b) return b+c*t end,
    linear = function(t,b,c,d) return b+c*t/d end,
    QuarticOut = function(t,b,c,d) return b+c*(1-(1-t/d)^4) end,
}}
local function read(p) local f=assert(io.open(p)); local s=f:read('a'); f:close(); return s end
local columns = {}
local function put(k,v)
    if type(v)=='table' then for j,x in ipairs(v) do put(k..'#'..(j-1),x) end; return end
    columns[k] = columns[k] or {}; table.insert(columns[k], string.format('%.8g',v))
end
local function evaluate(curve, t, a, b, d)
    if type(a)=='table' then local v={}; for j=1,#a do v[j]=curve(t,a[j],b[j]-a[j],d) end; return v end
    return curve(t,a,b-a,d)
end
if mode=='seek' then
    local s=read(path):gsub('return exports', 'return {tools=AETools, attrs=ae_attribute}')
    local data=assert(load(s))(); local tools=data.tools:new(data.attrs)
    for i=0,n-1 do for name in pairs(data.attrs) do put(name,tools:GetVal(name,i/(n-1)*duration)) end end
elseif mode=='ae' then
    local dir=path:match('(.+)/[^/]+$'); package.path=dir..'/?.lua;'..package.path
    local adapter=require('common/AEAdapter'):new(); local data=dofile(path); adapter:addKeyframes('',data)
    for i=0,n-1 do for name in pairs(data) do put(name,adapter:get('/'..name,i/(n-1)*duration)) end end
elseif mode=='distort' then
    local s=read(path); local stop=assert(s:find('    for i = 1, #self.params do',1,true))
    s=s:sub(1,stop-1)..'end\nreturn SeekModeScript'
    local cls=assert(load(s))(); local obj=cls.new(); obj.animDirty=true; obj.values={}; obj:updateAnim()
    for i=0,n-1 do
        local t=i/(n-1); local values={}
        for _,a in ipairs(obj.params) do
            if t>=a.startTime and t<=a.endTime then values[a.key]=evaluate(a.curve,t-a.startTime,a.startValue,a.endValue,a.endTime-a.startTime) end
        end
        for _,a in ipairs(obj.params) do if values[a.key]==nil then values[a.key]=a.defaultValue end end
        for k,v in pairs(values) do put(k,v) end
    end
else
    local s=read(path):gsub('return exports','return {class=Transform, move=funcEaseAction, blur=funcEaseBlurAction}')
    local data=assert(load(s))(); local obj=data.class.new()
    for i=0,n-1 do
        local t=i/(n-1); local values={position={0,0,0},scale={1,1,1},rotation={0,0,0},alpha=1,blur=0,blur_type=0}
        if obj.actions then
            for _,a in ipairs(obj.actions) do
                if t>=a.startTime and t<=a.endTime then
                    local dt=t-a.startTime; local d=a.endTime-a.startTime
                    for _,p in ipairs({{'Position','position'},{'Scale','scale'},{'Rotate','rotation'},{'Alpha','alpha'}}) do
                        if a['start'..p[1]] then values[p[2]]=evaluate(a.actionFunction,dt,a['start'..p[1]],a['end'..p[1]],d) end
                    end
                    if a.blurType then values.blur=a.actionFunction(dt,a.blurIntensity/(duration*d),-a.blurIntensity/(duration*d),d); values.blur_type=a.blurType end
                end
            end
        else
            local phase=obj.startPoint1 and (t<=obj.blurDuration1 and '1' or '2') or ''
            local d=phase=='' and 1 or obj['blurDuration'..phase]
            local dt=t-(phase=='2' and obj.blurDuration1 or 0)
            local move=obj['moveFunction'..phase] or (path:find('674086') and Amaz.Ease.QuarticOut or data.move)
            for _,p in ipairs({{'Point','position'},{'Scale','scale'},{'Rotate','rotation'}}) do
                values[p[2]]=evaluate(move,dt,obj['start'..p[1]..phase],obj['end'..p[1]..phase],d)
            end
            local blur=obj['blurFunction'..phase] or data.blur
            values.blur=blur(dt,obj['blurIntensity'..phase]/(duration*d),-obj['blurIntensity'..phase]/(duration*d),d)
            values.blur_type=obj['blurType'..phase] or (phase=='' and (path:find('674086') and 2 or 1) or 2)
        end
        for k,v in pairs(values) do put(k,v) end
    end
end
local out={}
for k,v in pairs(columns) do out[#out+1]=string.format('%q:[%s]',k,table.concat(v,',')) end
print('{'..table.concat(out,',')..'}')
