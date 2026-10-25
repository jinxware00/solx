-- Resolve only the sibling whose bytes are recorded by the harness. Lua's
-- ordinary require cache still handles every import after the first one.
local directory = assert(arg[0]:match("^(.*[/\\])"), "expected an absolute fixture path")
package.path = directory .. "?.lua"
package.cpath = ""
local sum = 0
for i = 1, 5000 do
    local data = require("import_target")
    sum = sum + data.next(i)
end
print(sum)
