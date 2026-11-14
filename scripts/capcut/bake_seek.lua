-- usage: lua bake_seek.lua <SeekModeScript.lua> <samples> -> JSON of every track over t in [0,1]
Amaz = setmetatable({}, {__index = function() return {} end})
local f = io.open(arg[1]); local text = f:read("a"); f:close()
text = text:gsub("local exports = exports or {}", "do return AETools, ae_attribute end\n%0", 1)
local AETools, attrs = load(text)()
local tools = AETools:new(attrs)
local n = tonumber(arg[2])
local out = {}
for name in pairs(attrs) do
  local col = {}
  for i = 0, n - 1 do
    local t = i / (n - 1)
    col[#col+1] = string.format("%.6g", tools:GetVal(name, t * 40 / 39)[1])
  end
  out[#out+1] = string.format('"%s":[%s]', name, table.concat(col, ","))
end
print("{" .. table.concat(out, ",") .. "}")
