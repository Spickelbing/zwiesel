This is a simple asynchronous TCP server for a user-defined protocol, along with a matching client.
The purpose of this is to keep me from writing boilerplate network communication code for small apps or games.
You just need to implement serialization and deserialization methods for your protocol, then you can get going.
[Here](examples/ex1.rs)'s an example.
