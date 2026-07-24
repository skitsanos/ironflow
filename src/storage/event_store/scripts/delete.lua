if #KEYS ~= 6 then
    return redis.error_reply('IRONFLOW_INVALID_EVENT_DELETE_KEYS')
end
if #ARGV ~= 2 then
    return redis.error_reply('IRONFLOW_INVALID_EVENT_DELETE_ARGUMENTS')
end
for left = 1, 6 do
    for right = left + 1, 6 do
        if KEYS[left] == KEYS[right] then
            return redis.error_reply('IRONFLOW_EVENT_KEYS_MUST_BE_DISTINCT')
        end
    end
end

local ttl = ARGV[2]
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
if fence_type == 'string' and redis.call('GET', KEYS[5]) ~= ARGV[1] then
    return redis.error_reply('IRONFLOW_EVENT_DELETION_FENCE_OWNER_MISMATCH')
end

local list_len = redis.call('LLEN', KEYS[1])
local index_len = redis.call('HLEN', KEYS[2])
local raw_seq = redis.call('GET', KEYS[3])
local seq = 0
if raw_seq ~= false then
    if raw_seq ~= '0' and string.match(raw_seq, '^[1-9][0-9]*$') == nil then
        return redis.error_reply('IRONFLOW_INVALID_EVENT_SEQUENCE')
    end
    seq = tonumber(raw_seq)
    if seq == nil or seq < 0 or seq ~= math.floor(seq) or seq > 9007199254740990 then
        return redis.error_reply('IRONFLOW_INVALID_EVENT_SEQUENCE')
    end
end
if seq ~= list_len or seq ~= index_len then
    return redis.error_reply('IRONFLOW_INCONSISTENT_EVENT_STORE')
end

local layout = redis.call('HGET', KEYS[4], 'layout')
local owner = redis.call('HGET', KEYS[4], 'run_id')
if layout ~= false and layout ~= '2' then
    return redis.error_reply('IRONFLOW_UNSUPPORTED_EVENT_LAYOUT')
end
if layout == '2' and owner ~= ARGV[1] then
    return redis.error_reply('IRONFLOW_EVENT_LAYOUT_OWNER_MISMATCH')
end
if layout == false and list_len > 0 then
    return redis.error_reply('IRONFLOW_UNVALIDATED_LEGACY_EVENT_LAYOUT')
end

-- Exercise command availability and ACL permission before installing the
-- deletion fence. The caller supplies a unique key and this type check makes a
-- collision fail closed instead of deleting unrelated data. Redis executes the
-- script atomically, so command permissions cannot change between this probe
-- and the namespace removal below.
if redis.call('TYPE', KEYS[6]).ok ~= 'none' then
    return redis.error_reply('IRONFLOW_EVENT_DELETE_PROBE_COLLISION')
end
if redis.call('UNLINK', KEYS[6]) ~= 0 then
    return redis.error_reply('IRONFLOW_EVENT_DELETE_PROBE_NOT_EMPTY')
end

-- A same-owner retry preserves the original fence lifetime. Extending the TTL
-- on every retry could turn periodic cleanup calls into an accidental
-- permanent tombstone.
if fence_type == 'none' then
    if ttl ~= '-1' then
        redis.call('SET', KEYS[5], ARGV[1], 'EX', ttl)
    else
        redis.call('SET', KEYS[5], ARGV[1])
    end
end
-- Remove the namespace atomically but reclaim large list/hash allocations on
-- Redis' lazy-free worker instead of blocking the server event loop.
redis.call('UNLINK', KEYS[1], KEYS[2], KEYS[3], KEYS[4])
return list_len
