local ttl = ARGV[6]
if #KEYS ~= 1 and #KEYS ~= 9 then
    return redis.error_reply('IRONFLOW_INVALID_CATALOG_KEYS')
end
if ttl ~= '-1' then
    local parsed_ttl = tonumber(ttl)
    if string.match(ttl, '^[1-9]%d*$') == nil or parsed_ttl == nil or parsed_ttl > 99999999999 then
        return redis.error_reply('IRONFLOW_INVALID_TTL')
    end
end
local run_type = redis.call('TYPE', KEYS[1]).ok
if run_type == 'none' then
    return redis.error_reply('IRONFLOW_RUN_NOT_FOUND')
end
if run_type ~= 'hash' then
    return redis.error_reply('IRONFLOW_INVALID_RUN_TYPE')
end
if redis.call('HEXISTS', KEYS[1], 'info') == 0 then
    return redis.error_reply('IRONFLOW_RUN_INFO_MISSING')
end

local catalog_enabled = #KEYS == 9
local status_key = nil
if catalog_enabled then
    local members_type = redis.call('TYPE', KEYS[2]).ok
    if members_type ~= 'none' and members_type ~= 'hash' then
        return redis.error_reply('IRONFLOW_INVALID_ORDERED_MEMBERS_TYPE')
    end
    for index = 3, 9 do
        local ordered_type = redis.call('TYPE', KEYS[index]).ok
        if ordered_type ~= 'none' and ordered_type ~= 'zset' then
            return redis.error_reply('IRONFLOW_INVALID_ORDERED_INDEX_TYPE')
        end
    end
    local status_keys = {
        pending = 4,
        running = 5,
        success = 6,
        failed = 7,
        stalled = 8,
        cancelled = 9
    }
    status_key = status_keys[ARGV[10]]
    if status_key == nil or ARGV[8] == nil or ARGV[9] == nil then
        return redis.error_reply('IRONFLOW_INVALID_ORDERED_STATUS')
    end
end

local incarnation = redis.call('HGET', KEYS[1], 'incarnation')
if incarnation == false then
    incarnation = ARGV[3]
end
if incarnation ~= ARGV[3] then
    return -1
end

local revision = redis.call('HGET', KEYS[1], 'revision')
if revision == false then
    revision = ARGV[1]
end
if revision ~= ARGV[2] then
    return 0
end

redis.call(
    'HSET', KEYS[1],
    'info', ARGV[4],
    'summary', ARGV[5],
    'revision', ARGV[7],
    'incarnation', ARGV[3]
)
if catalog_enabled then
    local old_member = redis.call('HGET', KEYS[2], ARGV[8])
    if old_member ~= false then
        for index = 3, 9 do
            redis.call('ZREM', KEYS[index], old_member)
        end
    end
    redis.call('HSET', KEYS[2], ARGV[8], ARGV[9])
    redis.call('ZADD', KEYS[3], 0, ARGV[9])
    redis.call('ZADD', KEYS[status_key], 0, ARGV[9])
end
if ttl ~= '-1' then
    redis.call('EXPIRE', KEYS[1], ttl)
else
    redis.call('PERSIST', KEYS[1])
end
return 1
