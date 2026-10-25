local function counter()
    local count = 0
    return function() count = count + 1 return count end
end
local next = counter()
local sum = 0
for i = 1, 10000 do sum = sum + next() end
print(sum)
