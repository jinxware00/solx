local i = 0
local sum = 0
while i < 20000 do
    i = i + 1
    if i % 2 == 1 then sum = sum + i end
end
print(sum)
