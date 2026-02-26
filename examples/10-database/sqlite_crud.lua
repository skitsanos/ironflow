--
-- SQLite CRUD operations
--
-- Demonstrates db_exec and db_query nodes with a file-based SQLite database.
-- Each step connects to the same file, so the data persists between steps.
--

local flow = Flow.new("sqlite_crud")

local db = "sqlite:/tmp/ironflow_test.db?mode=rwc"

-- Create table
flow:step("create_table", nodes.db_exec({
    connection = db,
    query = "CREATE TABLE IF NOT EXISTS users (id INTEGER PRIMARY KEY, name TEXT, email TEXT)"
}))

-- Insert rows
flow:step("insert_alice", nodes.db_exec({
    connection = db,
    query = "INSERT INTO users (name, email) VALUES (?, ?)",
    params = { "Alice", "alice@example.com" }
})):depends_on("create_table")

flow:step("insert_bob", nodes.db_exec({
    connection = db,
    query = "INSERT INTO users (name, email) VALUES (?, ?)",
    params = { "Bob", "bob@example.com" }
})):depends_on("create_table")

-- Query all rows
flow:step("query_all", nodes.db_query({
    connection = db,
    query = "SELECT * FROM users",
    output_key = "users"
})):depends_on("insert_alice"):depends_on("insert_bob")

flow:step("done", nodes.log({
    message = "Found ${ctx.users_count} users"
})):depends_on("query_all")

return flow
