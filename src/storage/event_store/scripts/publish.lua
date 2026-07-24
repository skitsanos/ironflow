if #KEYS ~= 5 then
    return redis.error_reply('IRONFLOW_INVALID_EVENT_PUBLISH_KEYS')
end
if #ARGV ~= 4 then
    return redis.error_reply('IRONFLOW_INVALID_EVENT_PUBLISH_ARGUMENTS')
end
for left = 1, 5 do
    for right = left + 1, 5 do
        if KEYS[left] == KEYS[right] then
            return redis.error_reply('IRONFLOW_EVENT_KEYS_MUST_BE_DISTINCT')
        end
    end
end

local ttl = ARGV[4]
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
if fence_type == 'string' then
    if redis.call('GET', KEYS[5]) ~= ARGV[3] then
        return redis.error_reply('IRONFLOW_EVENT_DELETION_FENCE_OWNER_MISMATCH')
    end
    return redis.error_reply('IRONFLOW_EVENT_STREAM_DELETED')
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
local ok, decoded = pcall(cjson.decode, ARGV[1])
if not ok or type(decoded) ~= 'table' or ARGV[2] == '' or decoded['id'] ~= ARGV[2] or decoded['run_id'] ~= ARGV[3] then
    return redis.error_reply('IRONFLOW_INVALID_EVENT_PAYLOAD')
end

local layout = redis.call('HGET', KEYS[4], 'layout')
if layout ~= false and layout ~= '2' then
    return redis.error_reply('IRONFLOW_UNSUPPORTED_EVENT_LAYOUT')
end
if layout == false and list_len > 0 then
    return redis.error_reply('IRONFLOW_UNVALIDATED_LEGACY_EVENT_LAYOUT')
end
if layout == '2' and redis.call('HGET', KEYS[4], 'run_id') ~= ARGV[3] then
    return redis.error_reply('IRONFLOW_EVENT_LAYOUT_OWNER_MISMATCH')
end

local existing = redis.call('HGET', KEYS[2], ARGV[2])
if existing ~= false then
    local position = tonumber(existing)
    if position == nil or position < 1 or position > list_len or position ~= math.floor(position) then
        return redis.error_reply('IRONFLOW_INVALID_EVENT_CURSOR')
    end
    if redis.call('LINDEX', KEYS[1], position - 1) ~= ARGV[1] then
        return redis.error_reply('IRONFLOW_EVENT_ID_CONFLICT')
    end
    if ttl ~= '-1' then
        redis.call('EXPIRE', KEYS[1], ttl)
        redis.call('EXPIRE', KEYS[2], ttl)
        redis.call('EXPIRE', KEYS[3], ttl)
        redis.call('EXPIRE', KEYS[4], ttl)
    else
        redis.call('PERSIST', KEYS[1])
        redis.call('PERSIST', KEYS[2])
        redis.call('PERSIST', KEYS[3])
        redis.call('PERSIST', KEYS[4])
    end
    return position
end

if seq == 9007199254740990 then
    return redis.error_reply('IRONFLOW_EVENT_SEQUENCE_EXHAUSTED')
end
redis.call('HSET', KEYS[4], 'layout', '2', 'run_id', ARGV[3])
local next_seq = redis.call('INCR', KEYS[3])
redis.call('RPUSH', KEYS[1], ARGV[1])
redis.call('HSET', KEYS[2], ARGV[2], tostring(next_seq))
if ttl ~= '-1' then
    redis.call('EXPIRE', KEYS[1], ttl)
    redis.call('EXPIRE', KEYS[2], ttl)
    redis.call('EXPIRE', KEYS[3], ttl)
    redis.call('EXPIRE', KEYS[4], ttl)
else
    redis.call('PERSIST', KEYS[1])
    redis.call('PERSIST', KEYS[2])
    redis.call('PERSIST', KEYS[3])
    redis.call('PERSIST', KEYS[4])
end
return next_seq
