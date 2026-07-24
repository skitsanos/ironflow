if #KEYS ~= 15 then
    return redis.error_reply('IRONFLOW_INVALID_EVENT_MIGRATION_KEYS')
end
if #ARGV ~= 6 then
    return redis.error_reply('IRONFLOW_INVALID_EVENT_MIGRATION_ARGUMENTS')
end

local owner = ARGV[1]
local token = ARGV[2]
local generation = ARGV[3]
local expected_phase = ARGV[4]
local expected_cursor = ARGV[5]
local expected_digest = ARGV[6]
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
if progress.cursor == progress.sequence then
    return {'done'}
end

local batch, status = ironflow_read_batch(progress, owner, false)
if batch == nil then
    if status == 'expired' or status == 'blocked' then
        return {'blocked'}
    end
    return {'invalid', status}
end
if batch.count == 0 then
    return redis.error_reply('IRONFLOW_EVENT_MIGRATION_EMPTY_BATCH')
end
local response = {
    'chunk', progress.cursor_text, tostring(batch.next_cursor), batch.digest
}
for _, raw in ipairs(batch.payloads) do
    response[#response + 1] = raw
end
return response
