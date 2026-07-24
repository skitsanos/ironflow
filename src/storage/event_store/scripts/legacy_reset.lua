if #KEYS ~= 15 then
    return redis.error_reply('IRONFLOW_INVALID_EVENT_MIGRATION_KEYS')
end
if #ARGV ~= 7 then
    return redis.error_reply('IRONFLOW_INVALID_EVENT_MIGRATION_ARGUMENTS')
end

local owner = ARGV[1]
local token = ARGV[2]
local generation = ARGV[3]
local expected_phase = ARGV[4]
local expected_cursor = ARGV[5]
local expected_digest = ARGV[6]
local failure_code = ARGV[7]
if failure_code == '' or string.len(failure_code) > 64
    or string.match(failure_code, '^[a-z_]+$') == nil then
    return redis.error_reply('IRONFLOW_INVALID_EVENT_MIGRATION_FAILURE')
end
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
local snapshot, status = ironflow_snapshot(progress)
if snapshot == nil then
    return {status == 'expired' and 'blocked' or status}
end

local next_generation = progress.generation + 1
redis.call('HSET', KEYS[9],
    'phase', 'restore',
    'generation', tostring(next_generation),
    'cursor', progress.cursor_text,
    'failure_code', failure_code)
progress.phase = 'restore'
progress.generation = next_generation
progress.generation_text = tostring(next_generation)
progress.failure_code = failure_code
return ironflow_progress_response(progress)
