use rocket_db_pools::deadpool_redis::Pool;
use rocket_db_pools::Database;

#[derive(Database)]
#[database("tasks")]
pub struct Tasks(Pool);
