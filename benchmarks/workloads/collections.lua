local values = {}
for i = 1, 5000 do
    table.insert(values, i % 97)
end
local state = {total = 0, count = 0}
for i = 1, 5000 do
    state.total = state.total + values[i]
    state.count = state.count + 1
end
print(state.total + state.count)
