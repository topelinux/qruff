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
let my_pipe = qruff.createCmdPipe();
cmd_generator.pipe(my_pipe);
//
//let consume = create_consume();
//
//my_pipe.pipe(consume);
//
console.log(cmd_generator.run());
my_pipe.show();
//console.log(cmd_generator.run());
