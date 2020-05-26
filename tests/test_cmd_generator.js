import * as qruff from "qruff";

let setTimeout = qruff.setTimeout;
//let a = setTimeout(() => {
//    console.log('Cool Qruff timer 1000ms be triggerd');
//}, 5000);

let cmd_generator = qruff.createCmdGenerator(JSON.stringify(
[
    {id: 'getTemperature', reg_offset: 0x3, reg_len:1, interval: 1000},
    {id: 'getHumit', reg_offset: 0x5, reg_len:1, interval: 2000},
]
));

console.log(cmd_generator.CONST_16);
let my_endpoint = qruff.createCmdEndpoint();
// clone my_endpoint input(channel.tx) to cmd_generator
cmd_generator.endpoint(my_endpoint);

// move cmd_generator.rx to my_endpoint
//
//let consume = create_consume();
//
//my_endpoint.endpoint(consume);
//
console.log(cmd_generator.run());
my_endpoint.show();
//console.log(cmd_generator.run());
