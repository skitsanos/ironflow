if #KEYS ~= 15 then
    return redis.error_reply('IRONFLOW_INVALID_EVENT_MIGRATION_KEYS')
end
if #ARGV ~= 8 then
    return redis.error_reply('IRONFLOW_INVALID_EVENT_MIGRATION_ARGUMENTS')
end

local owner = ARGV[1]
local token = ARGV[2]
local generation = ARGV[3]
local expected_phase = ARGV[4]
local expected_cursor = ARGV[5]
local expected_digest = ARGV[6]
local expected_next_cursor = ARGV[7]
local expected_batch_digest = ARGV[8]
if ironflow_key_type(KEYS[9]) == 'none' then
    return {'stale'}
end
local progress = ironflow_load_progress(owner, token)
if progress == nil
    or not ironflow_matches(
        progress, token, generation, expected_phase, expected_cursor, expected_digest) then
    return {'stale'}
end
if progress.phase ~= 'scan' and progress.phase ~= 'verify' then
    return {'stale'}
end
ironflow_assert_key_layout(progress.mode)
if not ironflow_sources_absent(progress) then
    return {'blocked'}
end

local batch, status = ironflow_read_batch(progress, owner, false)
if batch == nil then
    if status == 'expired' or status == 'blocked' then
        return {'blocked'}
    end
    return {'invalid', status}
end
if tostring(batch.next_cursor) ~= expected_next_cursor
    or batch.digest ~= expected_batch_digest then
    return {'changed'}
end

ironflow_preflight_lmove()
local pending_phase = progress.phase .. '_pending'
local next_digest = ironflow_batch_rolling_digest(progress, batch)
local next_generation = progress.generation + 1
redis.call('HSET', KEYS[9],
    'phase', pending_phase,
    'generation', tostring(next_generation),
    'pending_count', tostring(batch.count),
    'pending_digest', batch.digest,
    'pending_next_cursor', tostring(batch.next_cursor),
    'pending_next_digest', next_digest)

-- No fallible metadata command may follow the rotations. A client disconnect
-- still lets Redis finish the script; a command-level failure leaves the
-- persisted pending intent for a fail-closed retry.
for _ = 1, batch.count do
    redis.call('LMOVE', KEYS[10], KEYS[10], 'LEFT', 'RIGHT')
end
return {'pending', tostring(next_generation)}
