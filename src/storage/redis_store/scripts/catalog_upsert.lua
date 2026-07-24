if #KEYS ~= 9 then
    return redis.error_reply('IRONFLOW_INVALID_CATALOG_KEYS')
end

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
local status_key = status_keys[ARGV[6]]
if status_key == nil then
    return redis.error_reply('IRONFLOW_INVALID_ORDERED_STATUS')
end

local run_type = redis.call('TYPE', KEYS[1]).ok
if run_type == 'none' then
    return 0
end
if run_type ~= 'hash' or redis.call('HEXISTS', KEYS[1], 'info') == 0 then
    return redis.error_reply('IRONFLOW_INVALID_RUN_TYPE')
end

local raw_info = redis.call('HGET', KEYS[1], 'info')
local info_ok, info = pcall(cjson.decode, raw_info)
local summary_ok, summary = pcall(cjson.decode, ARGV[3])
if not info_ok or type(info) ~= 'table' or info['id'] ~= ARGV[4]
    or not summary_ok or type(summary) ~= 'table' or summary['id'] ~= ARGV[4]
    or summary['status'] ~= ARGV[6] then
    return redis.error_reply('IRONFLOW_CURRENT_RUN_OWNER_MISMATCH')
end

local revision = redis.call('HGET', KEYS[1], 'revision')
if revision == false then
    revision = ARGV[1]
end
if revision ~= ARGV[2] then
    return 2
end

local old_member = redis.call('HGET', KEYS[2], ARGV[4])
if old_member ~= false then
    for index = 3, 9 do
        redis.call('ZREM', KEYS[index], old_member)
    end
end
redis.call('HSET', KEYS[1], 'summary', ARGV[3])
redis.call('HSET', KEYS[2], ARGV[4], ARGV[5])
redis.call('ZADD', KEYS[3], 0, ARGV[5])
redis.call('ZADD', KEYS[status_key], 0, ARGV[5])
return 1
