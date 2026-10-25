local sum = 0
for i = 1, 10000 do
    local items = {i, i + 1}
    local record = {items = items}
    sum = sum + record.items[2]
end
print(sum)
