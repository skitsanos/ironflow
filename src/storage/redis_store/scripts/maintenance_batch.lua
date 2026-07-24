if #KEYS ~= 3 then
    return redis.error_reply('IRONFLOW_INVALID_MAINTENANCE_KEYS')
end
for left = 1, 3 do
    for right = left + 1, 3 do
        if KEYS[left] == KEYS[right] then
            return redis.error_reply('IRONFLOW_INVALID_MAINTENANCE_KEYS')
        end
    end
end

local count = tonumber(ARGV[1])
if count == nil or count < 1 or count > 1024 or count ~= math.floor(count) then
    return redis.error_reply('IRONFLOW_INVALID_MAINTENANCE_BATCH_SIZE')
end

local catalog_type = redis.call('TYPE', KEYS[1]).ok
if catalog_type ~= 'none' and catalog_type ~= 'zset' then
    return redis.error_reply('IRONFLOW_INVALID_ORDERED_INDEX_TYPE')
end
local cursor_type = redis.call('TYPE', KEYS[2]).ok
if cursor_type ~= 'none' and cursor_type ~= 'string' then
    return redis.error_reply('IRONFLOW_INVALID_MAINTENANCE_CURSOR_TYPE')
end
local high_water_type = redis.call('TYPE', KEYS[3]).ok
if high_water_type ~= 'none' and high_water_type ~= 'string' then
    return redis.error_reply('IRONFLOW_INVALID_MAINTENANCE_HIGH_WATER_TYPE')
end

local cursor = redis.call('GET', KEYS[2])
local high_water = redis.call('GET', KEYS[3])
if high_water == false then
    local newest = redis.call('ZREVRANGE', KEYS[1], 0, 0)
    if #newest == 0 then
        redis.call('DEL', KEYS[2], KEYS[3])
        return {}
    end
    -- A cursor without its cycle boundary can be left by an older IronFlow
    -- version or an interrupted upgrade. Start a complete bounded cycle.
    redis.call('DEL', KEYS[2])
    cursor = false
    high_water = newest[1]
    redis.call('SET', KEYS[3], high_water)
end

local minimum = '-'
if cursor ~= false then
    minimum = '(' .. cursor
end

local members = redis.call(
    'ZRANGEBYLEX', KEYS[1], minimum, '[' .. high_water, 'LIMIT', 0, count
)

if #members < count or (#members > 0 and members[#members] == high_water) then
    redis.call('DEL', KEYS[2], KEYS[3])
else
    redis.call('SET', KEYS[2], members[#members])
end
return members
