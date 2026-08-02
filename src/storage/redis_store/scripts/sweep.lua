if KEYS[1] == KEYS[2] then
    return redis.error_reply('IRONFLOW_STATE_KEY_ALIAS')
end
if #KEYS ~= 2 and #KEYS ~= 10 and #KEYS ~= 11 then
    return redis.error_reply('IRONFLOW_INVALID_CATALOG_KEYS')
end

local index_type = redis.call('TYPE', KEYS[2]).ok
if index_type ~= 'none' and index_type ~= 'set' then
    return redis.error_reply('IRONFLOW_INVALID_RUN_INDEX_TYPE')
end
local catalog_enabled = #KEYS == 10 or #KEYS == 11
local lease_index_enabled = #KEYS == 11
if catalog_enabled then
    local members_type = redis.call('TYPE', KEYS[3]).ok
    if members_type ~= 'none' and members_type ~= 'hash' then
        return redis.error_reply('IRONFLOW_INVALID_ORDERED_MEMBERS_TYPE')
    end
    for index = 4, 10 do
        local ordered_type = redis.call('TYPE', KEYS[index]).ok
        if ordered_type ~= 'none' and ordered_type ~= 'zset' then
            return redis.error_reply('IRONFLOW_INVALID_ORDERED_INDEX_TYPE')
        end
    end
end
if redis.call('EXISTS', KEYS[1]) ~= 0 then
    return 0
end
redis.call('SREM', KEYS[2], ARGV[1])
if lease_index_enabled then redis.call('ZREM', KEYS[11], ARGV[1]) end
if catalog_enabled then
    local old_member = redis.call('HGET', KEYS[3], ARGV[1])
    if old_member ~= false then
        for index = 4, 10 do
            redis.call('ZREM', KEYS[index], old_member)
        end
    end
    redis.call('HDEL', KEYS[3], ARGV[1])
end
return 1
