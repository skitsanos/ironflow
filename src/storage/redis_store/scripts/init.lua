if KEYS[1] == KEYS[2] then
    return redis.error_reply('IRONFLOW_STATE_KEY_ALIAS')
end
if #KEYS ~= 2 and #KEYS ~= 11 and #KEYS ~= 12 then
    return redis.error_reply('IRONFLOW_INVALID_CATALOG_KEYS')
end

local ttl = ARGV[6]
local parsed_ttl = nil
if ttl ~= '-1' then
    parsed_ttl = tonumber(ttl)
    if string.match(ttl, '^[1-9]%d*$') == nil or parsed_ttl == nil or parsed_ttl > 99999999999 then
        return redis.error_reply('IRONFLOW_INVALID_TTL')
    end
end
local run_type = redis.call('TYPE', KEYS[1]).ok
local index_type = redis.call('TYPE', KEYS[2]).ok
if run_type == 'hash' then
    local raw_info = redis.call('HGET', KEYS[1], 'info')
    local info_ok, info = pcall(cjson.decode, raw_info)
    if not info_ok or type(info) ~= 'table' or info['id'] ~= ARGV[5] then
        return redis.error_reply('IRONFLOW_CURRENT_RUN_OWNER_MISMATCH')
    end
    return 0
end
if run_type ~= 'none' then
    return redis.error_reply('IRONFLOW_INVALID_RUN_TYPE')
end
if index_type ~= 'none' and index_type ~= 'set' then
    return redis.error_reply('IRONFLOW_INVALID_RUN_INDEX_TYPE')
end

local catalog_enabled = #KEYS == 11 or #KEYS == 12
local lease_index_enabled = #KEYS == 12
local status_key = nil
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
    local ready_type = redis.call('TYPE', KEYS[11]).ok
    if ready_type ~= 'none' and ready_type ~= 'string' then
        return redis.error_reply('IRONFLOW_INVALID_ORDERED_READY_TYPE')
    end
    local status_keys = {
        pending = 5,
        running = 6,
        success = 7,
        failed = 8,
        stalled = 9,
        cancelled = 10
    }
    status_key = status_keys[ARGV[8]]
    if status_key == nil or ARGV[7] == nil then
        return redis.error_reply('IRONFLOW_INVALID_ORDERED_STATUS')
    end
end
if lease_index_enabled then
    local lease_index_type = redis.call('TYPE', KEYS[12]).ok
    if lease_index_type ~= 'none' and lease_index_type ~= 'zset' then
        return redis.error_reply('IRONFLOW_INVALID_RUN_LEASE_INDEX_TYPE')
    end
    if ARGV[9] ~= '' then
        if string.match(ARGV[10], '^[1-9]%d*$') == nil or tonumber(ARGV[10]) == nil then
            return redis.error_reply('IRONFLOW_INVALID_RUN_LEASE_TTL')
        end
        local safety = ARGV[11] or '0'
        if string.match(safety, '^%d+$') == nil or tonumber(safety) == nil then
            return redis.error_reply('IRONFLOW_INVALID_RUN_LEASE_SAFETY')
        end
    end
end

redis.call(
    'HSET', KEYS[1],
    'info', ARGV[1],
    'summary', ARGV[2],
    'revision', ARGV[3],
    'incarnation', ARGV[4]
)
redis.call('SADD', KEYS[2], ARGV[5])
if lease_index_enabled and ARGV[9] ~= '' then
    local time = redis.call('TIME')
    local now = (tonumber(time[1]) * 1000000) + tonumber(time[2])
    local expiry = string.format('%.0f', now + tonumber(ARGV[10]))
    redis.call('HSET', KEYS[1], 'lease_owner', ARGV[9], 'lease_expires_micros', expiry)
    redis.call('ZADD', KEYS[12], expiry, ARGV[5])
end
if catalog_enabled then
    local old_member = redis.call('HGET', KEYS[3], ARGV[5])
    if old_member ~= false then
        for index = 4, 10 do
            redis.call('ZREM', KEYS[index], old_member)
        end
    end
    redis.call('HSET', KEYS[3], ARGV[5], ARGV[7])
    redis.call('ZADD', KEYS[4], 0, ARGV[7])
    redis.call('ZADD', KEYS[status_key], 0, ARGV[7])
    if index_type == 'none' then
        redis.call('SET', KEYS[11], '1')
    end
end
if lease_index_enabled and ARGV[9] ~= '' and parsed_ttl ~= nil then
    local safety = tonumber(ARGV[11] or '0')
    local active_ttl = math.ceil((tonumber(ARGV[10]) + safety) / 1000000)
    active_ttl = math.max(active_ttl, parsed_ttl)
    redis.call('EXPIRE', KEYS[1], active_ttl)
elseif parsed_ttl ~= nil then
    redis.call('EXPIRE', KEYS[1], parsed_ttl)
else
    redis.call('PERSIST', KEYS[1])
end
return 1
