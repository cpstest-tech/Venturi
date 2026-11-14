-- usage: lua bake_text.lua <text animation lua/src> <samples>
Amaz = setmetatable({}, {__index = function() return function() end end})
package.path = arg[1] .. '/?.lua;' .. package.path
local AE = require('AE')
local Tools = require('common/AETools')
local out = {}
for name, data in pairs(AE) do
    local ae = Tools.new(data)
    local track = name == 'scale' and 'Scale' or 'Position'
    ae:setAnimFrameRange(0, name == 'scale' and 23 or 24)
    local values = {}
    for i = 0, tonumber(arg[2])-1 do
        local value = ae:GetVal(track, i / (tonumber(arg[2])-1))[1]
        values[#values+1] = string.format('%.7g', value)
    end
    out[#out+1] = string.format('"%s":[%s]', name, table.concat(values, ','))
end
print('{' .. table.concat(out, ',') .. '}')
