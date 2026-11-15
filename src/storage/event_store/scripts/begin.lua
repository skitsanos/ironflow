-- Record one run incarnation as the publisher of its event stream (IF-184,
-- IF-214).
-- KEYS[1..4] stream list, index, sequence, meta; KEYS[5] fence; KEYS[6]
-- admitted incarnation. ARGV[1] run ID, ARGV[2] incarnation, ARGV[3] TTL in
-- seconds or -1. Returns 0 when the incarnation was already admitted, 1 when
-- a closed fence was reopened and 2 when a new admission was recorded.
if #KEYS ~= 6 then
    return redis.error_reply('IRONFLOW_INVALID_EVENT_INCARNATION_KEYS')
end
for left = 1, 6 do
    for right = left + 1, 6 do
        if KEYS[left] == KEYS[right] then
            return redis.error_reply('IRONFLOW_EVENT_KEYS_MUST_BE_DISTINCT')
        end
    end
end
if #ARGV ~= 3 or ARGV[2] == '' then
    return redis.error_reply('IRONFLOW_INVALID_EVENT_INCARNATION_ARGUMENTS')
end
local ttl = ARGV[3]
if ttl ~= '-1' then
    if string.match(ttl, '^[1-9][0-9]*$') == nil or string.len(ttl) > 11 then
        return redis.error_reply('IRONFLOW_INVALID_TTL')
    end
end

local list_type = redis.call('TYPE', KEYS[1]).ok
local index_type = redis.call('TYPE', KEYS[2]).ok
local seq_type = redis.call('TYPE', KEYS[3]).ok
local meta_type = redis.call('TYPE', KEYS[4]).ok
local fence_type = redis.call('TYPE', KEYS[5]).ok
local admitted_type = redis.call('TYPE', KEYS[6]).ok
if list_type ~= 'none' and list_type ~= 'list' then
    return redis.error_reply('IRONFLOW_INVALID_EVENT_LIST_TYPE')
end
if index_type ~= 'none' and index_type ~= 'hash' then
    return redis.error_reply('IRONFLOW_INVALID_EVENT_INDEX_TYPE')
end
if seq_type ~= 'none' and seq_type ~= 'string' then
    return redis.error_reply('IRONFLOW_INVALID_EVENT_SEQUENCE_TYPE')
end
if meta_type ~= 'none' and meta_type ~= 'hash' then
    return redis.error_reply('IRONFLOW_INVALID_EVENT_META_TYPE')
end
if fence_type ~= 'none' and fence_type ~= 'string' then
    return redis.error_reply('IRONFLOW_INVALID_EVENT_DELETION_FENCE_TYPE')
end
if admitted_type ~= 'none' and admitted_type ~= 'string' then
    return redis.error_reply('IRONFLOW_INVALID_EVENT_INCARNATION_TYPE')
end
if fence_type == 'string' and redis.call('GET', KEYS[5]) ~= ARGV[1] then
    return redis.error_reply('IRONFLOW_EVENT_DELETION_FENCE_OWNER_MISMATCH')
end
if admitted_type == 'string' and redis.call('GET', KEYS[6]) == ARGV[2] then
    return 0
end

local result = 1
if fence_type == 'none' or admitted_type == 'string' then
    -- No closed fence: any retained events were published by an earlier
    -- incarnation whose state is gone, so they are orphans. The caller
    -- validated the layout; refuse a stream that is not this run's.
    local layout = false
    local owner = false
    if meta_type == 'hash' then
        layout = redis.call('HGET', KEYS[4], 'layout')
        owner = redis.call('HGET', KEYS[4], 'run_id')
    end
    if layout ~= false and layout ~= '2' then
        return redis.error_reply('IRONFLOW_UNSUPPORTED_EVENT_LAYOUT')
    end
    if layout == '2' and owner ~= ARGV[1] then
        return redis.error_reply('IRONFLOW_EVENT_LAYOUT_OWNER_MISMATCH')
    end
    if layout == false and redis.call('LLEN', KEYS[1]) > 0 then
        return redis.error_reply('IRONFLOW_UNVALIDATED_LEGACY_EVENT_LAYOUT')
    end
    redis.call('UNLINK', KEYS[1], KEYS[2], KEYS[3], KEYS[4])
    result = 2
end

if fence_type == 'string' then
    -- The admission shares the fence's remaining lifetime exactly: time does
    -- not advance inside a script, so the two keys expire together.
    local remaining = redis.call('PTTL', KEYS[5])
    if remaining > 0 then
        redis.call('SET', KEYS[6], ARGV[2], 'PX', remaining)
    else
        redis.call('SET', KEYS[6], ARGV[2])
    end
elseif ttl ~= '-1' then
    -- Without a fence the admission follows the stream's own retention;
    -- publication refreshes both.
    redis.call('SET', KEYS[6], ARGV[2], 'EX', ttl)
else
    redis.call('SET', KEYS[6], ARGV[2])
end
return result
