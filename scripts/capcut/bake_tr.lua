-- usage: lua bake_tr.lua <lua/src dir> <duration> <samples> <track>...  -> JSON
Amaz = setmetatable({}, {__index = function() return function() end end})
local dir, dur, n = arg[1], tonumber(arg[2]), tonumber(arg[3])
package.path = dir .. "/?.lua;" .. package.path
local AEAdapter = require("common/AEAdapter")
local AE = require("AE")
local ae = AEAdapter:new()
for layer, data in pairs(AE) do if type(data) == "table" then ae:addKeyframes(layer, data) end end
local out = {}
for k = 4, #arg do
  local cols = {}
  for i = 0, n - 1 do
    local v = ae:get(arg[k], dur * i / (n - 1))
    for j = 1, #v do cols[j] = cols[j] or {}; cols[j][#cols[j]+1] = string.format("%.6g", v[j]) end
  end
  for j = 1, #cols do out[#out+1] = string.format('"%s#%d":[%s]', arg[k], j-1, table.concat(cols[j], ",")) end
end
print("{" .. table.concat(out, ",") .. "}")
