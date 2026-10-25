local checksum = 0
for i = 1, 50000 do
    checksum = checksum + (i * 17) % 97
end
print(checksum)
