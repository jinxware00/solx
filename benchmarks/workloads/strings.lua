local checksum = 0
for i = 1, 4000 do
    local text = string.upper("solx") .. tostring(i % 10) .. "-runtime"
    checksum = checksum + string.len(text)
end
print(checksum)
