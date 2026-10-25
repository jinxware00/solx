local function step(value)
    return value * 3 + 1
end
local checksum = 0
for i = 1, 20000 do
    checksum = checksum + step(i)
end
print(checksum)
