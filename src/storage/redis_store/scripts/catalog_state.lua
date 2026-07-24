if #KEYS ~= 10 or #ARGV ~= 0 then
    return redis.error_reply('IRONFLOW_INVALID_CATALOG_STATE_ARITY')
end
for left = 1, 10 do
    for right = left + 1, 10 do
        if KEYS[left] == KEYS[right] then
            return redis.error_reply('IRONFLOW_CATALOG_KEYS_MUST_BE_DISTINCT')
        end
    end
end

local ready_type = redis.call('TYPE', KEYS[1]).ok
if ready_type == 'none' then
    return {'', 0}
end
if ready_type ~= 'string' then
    return redis.error_reply('IRONFLOW_INVALID_ORDERED_READY_TYPE')
end

local index_type = redis.call('TYPE', KEYS[2]).ok
local members_type = redis.call('TYPE', KEYS[3]).ok
if index_type ~= 'none' and index_type ~= 'set' then
    return redis.error_reply('IRONFLOW_INVALID_RUN_INDEX_TYPE')
end
if members_type ~= 'none' and members_type ~= 'hash' then
    return redis.error_reply('IRONFLOW_INVALID_ORDERED_MEMBERS_TYPE')
end
for position = 4, 10 do
    local ordered_type = redis.call('TYPE', KEYS[position]).ok
    if ordered_type ~= 'none' and ordered_type ~= 'zset' then
        return redis.error_reply('IRONFLOW_INVALID_ORDERED_INDEX_TYPE')
    end
end

local legacy = redis.call('SCARD', KEYS[2])
local members = redis.call('HLEN', KEYS[3])
local all = redis.call('ZCARD', KEYS[4])
local status_total = 0
for position = 5, 10 do
    status_total = status_total + redis.call('ZCARD', KEYS[position])
end
local consistent = 0
if legacy == members and members == all and all == status_total then
    consistent = 1
end
return {redis.call('GET', KEYS[1]), consistent}
