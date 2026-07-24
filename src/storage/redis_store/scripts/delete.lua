if KEYS[1] == KEYS[2] then
    return redis.error_reply('IRONFLOW_STATE_KEY_ALIAS')
end
if #KEYS ~= 2 and #KEYS ~= 10 then
    return redis.error_reply('IRONFLOW_INVALID_CATALOG_KEYS')
end

local index_type = redis.call('TYPE', KEYS[2]).ok
if index_type ~= 'none' and index_type ~= 'set' then
    return redis.error_reply('IRONFLOW_INVALID_RUN_INDEX_TYPE')
end
local catalog_enabled = #KEYS == 10
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

local function remove_catalog_entry()
    if not catalog_enabled then
        return
    end
    local old_member = redis.call('HGET', KEYS[3], ARGV[1])
    if old_member ~= false then
        for index = 4, 10 do
            redis.call('ZREM', KEYS[index], old_member)
        end
    end
    redis.call('HDEL', KEYS[3], ARGV[1])
end

local run_type = redis.call('TYPE', KEYS[1]).ok
if run_type == 'none' then
    redis.call('SREM', KEYS[2], ARGV[1])
    remove_catalog_entry()
    return 0
end
if run_type ~= 'hash' then
    return redis.error_reply('IRONFLOW_INVALID_RUN_TYPE')
end
local raw_info = redis.call('HGET', KEYS[1], 'info')
local info_ok, info = pcall(cjson.decode, raw_info)
if not info_ok or type(info) ~= 'table' or info['id'] ~= ARGV[1] then
    return redis.error_reply('IRONFLOW_CURRENT_RUN_OWNER_MISMATCH')
end
redis.call('DEL', KEYS[1])
redis.call('SREM', KEYS[2], ARGV[1])
remove_catalog_entry()
return 1
