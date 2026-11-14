-- usage: lua bake.lua <LumiFamily dir> <samples>  -> JSON {"comp":[a,b], "track#i":[...]}
Amaz = { Vector2f = function(...) return {...} end, Vector3f = function(...) return {...} end, Color = function(...) return {...} end }
local dir, n = arg[1], tonumber(arg[2])
local data = dofile(dir .. "/LumiExportData.lua")
local AETools = dofile(dir .. "/AETools.lua")
local tools = AETools.new(data.ae_keyframes)
local dur = data.ae_compDurations
local out = {}
out[#out+1] = string.format('"comp":[%.9g,%.9g]', dur[1], dur[2])
for name in pairs(data.ae_keyframes) do
  local cols = {}
  for i = 0, n - 1 do
    local t = dur[1] + (dur[2]-dur[1]) * i / (n - 1)
    local v = tools:GetVal(name, t)
    if type(v) ~= "table" then v = {v} end
    for j = 1, #v do cols[j] = cols[j] or {}; cols[j][#cols[j]+1] = string.format("%.6g", v[j]) end
  end
  for j = 1, #cols do out[#out+1] = string.format('"%s#%d":[%s]', name, j-1, table.concat(cols[j], ",")) end
end
print("{" .. table.concat(out, ",") .. "}")
