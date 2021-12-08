use std::path::Path;
use std::sync::mpsc;
use std::thread;

use grovedb::GroveDb;
use neon::prelude::*;

type DbCallback = Box<dyn FnOnce(&mut Connection, &Channel) + Send>;

// Wraps a SQLite connection a channel, allowing concurrent access
struct Database {
    tx: mpsc::Sender<DbMessage>,
}

// Messages sent on the database channel
enum DbMessage {
    // Callback to be executed
    Callback(DbCallback),
    // Indicates that the thread should be stopped and connection closed
    Close,
}

struct GroveDbWrapper {
    tx: mpsc::Sender<DbMessage>,
}

// Internal wrapper logic
impl GroveDbWrapper {
    // Creates a new instance of `Database`
    //
    // 1. Creates a connection and a channel
    // 2. Spawns a thread and moves the channel receiver and connection to it
    // 3. On a separate thread, read closures off the channel and execute with access
    //    to the connection.
    fn new<'a, C>(cx: &mut C) -> rusqlite::Result<Self>
    where
        C: Context<'a>,
    {
        let path = cx.argument::<JsString>(0)?.value();
        let path = Path::new(&path);

        // Channel for sending callbacks to execute on the GroveDb connection thread
        let (tx, rx) = mpsc::channel::<DbMessage>();

        // Open a connection to groveDb, this will be moved to a separate thread
        let mut grove_db = GroveDb::open(path)?;

        // Create an `Channel` for calling back to JavaScript. It is more efficient
        // to create a single channel and re-use it for all database callbacks.
        // The JavaScript process will not exit as long as this channel has not been
        // dropped.
        let channel = cx.channel();

        // Spawn a thread for processing database queries
        // This will not block the JavaScript main thread and will continue executing
        // concurrently.
        thread::spawn(move || {
            // Blocks until a callback is available
            // When the instance of `Database` is dropped, the channel will be closed
            // and `rx.recv()` will return an `Err`, ending the loop and terminating
            // the thread.
            while let Ok(message) = rx.recv() {
                match message {
                    DbMessage::Callback(f) => {
                        // The connection and channel are owned by the thread, but _lent_ to
                        // the callback. The callback has exclusive access to the connection
                        // for the duration of the callback.
                        f(&mut grove_db, &channel);
                    }
                    // Immediately close the connection, even if there are pending messages
                    DbMessage::Close => break,
                }
            }
        });

        Ok(Self { tx })
    }

    // Idiomatic rust would take an owned `self` to prevent use after close
    // However, it's not possible to prevent JavaScript from continuing to hold a closed database
    fn close(&self) -> Result<(), mpsc::SendError<DbMessage>> {
        self.tx.send(DbMessage::Close)
    }

    fn send(
        &self,
        callback: impl FnOnce(&mut Connection, &Channel) + Send + 'static,
    ) -> Result<(), mpsc::SendError<DbMessage>> {
        self.tx.send(DbMessage::Callback(Box::new(callback)))
    }
}

// Ensures that GroveDbWrapper is properly disposed when the corresponding JS object
// gets garbage collected
impl Finalize for GroveDbWrapper {}

// External wrapper logic
impl GroveDbWrapper {
    // Create a new instance of `Database` and place it inside a `JsBox`
    // JavaScript can hold a reference to a `JsBox`, but the contents are opaque
    fn js_open(mut cx: FunctionContext) -> JsResult<JsBox<GroveDbWrapper>> {
        let dbWrapper =
            GroveDbWrapper::new(&mut cx).or_else(|err| cx.throw_error(err.to_string()))?;

        Ok(cx.boxed(dbWrapper))
    }

    fn js_get(mut cx: FunctionContext) -> JsResult<JsUndefined> {
        // Get the first argument as a `JsNumber` and convert to an `f64`
        let id = cx.argument::<JsNumber>(0)?.value(&mut cx);

        // Get the second argument as a `JsFunction`
        let callback = cx.argument::<JsFunction>(1)?.root(&mut cx);

        // Get the `this` value as a `JsBox<Database>`
        let db = cx
            .this()
            .downcast_or_throw::<JsBox<GroveDbWrapper>, _>(&mut cx)?;

        db.send(move |grove_db: &mut GroveDb, channel| {
            let result: Result<String, _> = grove_db.get();

            channel.send(move |mut cx| {
                let callback = callback.into_inner(&mut cx);
                let this = cx.undefined();
                let args: Vec<Handle<JsValue>> = match result {
                    // Convert the name to a `JsString` on success and upcast to a `JsValue`
                    Ok(name) => vec![cx.null().upcast(), cx.string(name).upcast()],

                    // If the row was not found, return `undefined` as a success instead
                    // of throwing an exception
                    Err(rusqlite::Error::QueryReturnedNoRows) => {
                        vec![cx.null().upcast(), cx.undefined().upcast()]
                    }

                    // Convert the error to a JavaScript exception on failure
                    Err(err) => vec![cx.error(err.to_string())?.upcast()],
                };

                callback.call(&mut cx, this, args)?;

                Ok(())
            });
        })
        .or_else(|err| cx.throw_error(err.to_string()))?;

        // This function does not have a return value
        Ok(cx.undefined())
    }
}

#[neon::main]
fn main(mut cx: ModuleContext) -> NeonResult<()> {
    cx.export_function("groveDbOpen", GroveDbWrapper::js_open)?;
    cx.export_function("groveDbGet", GroveDbWrapper::js_get)?;

    Ok(())
}
