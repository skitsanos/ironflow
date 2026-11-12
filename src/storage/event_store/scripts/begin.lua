-- Admit one run incarnation past a retained deletion fence (IF-184).
-- KEYS[1] fence, KEYS[2] admitted incarnation. ARGV[1] run ID, ARGV[2]
-- incarnation. Returns 1 when a fence was reopened and 0 when none exists.
if #KEYS ~= 2 or KEYS[1] == KEYS[2] then
    return redis.error_reply('IRONFLOW_INVALID_EVENT_INCARNATION_KEYS')
end
if #ARGV ~= 2 or ARGV[2] == '' then
    return redis.error_reply('IRONFLOW_INVALID_EVENT_INCARNATION_ARGUMENTS')
end

local fence_type = redis.call('TYPE', KEYS[1]).ok
local admitted_type = redis.call('TYPE', KEYS[2]).ok
if fence_type ~= 'none' and fence_type ~= 'string' then
    return redis.error_reply('IRONFLOW_INVALID_EVENT_DELETION_FENCE_TYPE')
end
if admitted_type ~= 'none' and admitted_type ~= 'string' then
    return redis.error_reply('IRONFLOW_INVALID_EVENT_INCARNATION_TYPE')
end
if fence_type == 'none' then
    -- Never deleted, or the fence expired: every publisher is accepted and a
    -- stale admission must not outlive the fence it belonged to.
    redis.call('UNLINK', KEYS[2])
    return 0
end
if redis.call('GET', KEYS[1]) ~= ARGV[1] then
    return redis.error_reply('IRONFLOW_EVENT_DELETION_FENCE_OWNER_MISMATCH')
end

-- The admission shares the fence's remaining lifetime exactly: time does not
-- advance inside a script, so the two keys expire together.
local remaining = redis.call('PTTL', KEYS[1])
if remaining > 0 then
    redis.call('SET', KEYS[2], ARGV[2], 'PX', remaining)
else
    redis.call('SET', KEYS[2], ARGV[2])
end
return 1
