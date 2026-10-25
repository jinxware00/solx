-- Luau resolves relative module paths from this script instead of package.path.
local sum = 0
for i = 1, 5000 do
    local data = require("./import_target")
    sum = sum + data.next(i)
end
print(sum)
