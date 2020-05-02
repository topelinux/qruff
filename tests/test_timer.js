import * as std from "std";
import * as os from "os";
import * as qruff from "qruff";

function test_timer()
{
    var th, i;

    /* just test that a timer can be inserted and removed */
    th = [];
    for(i = 0; i < 3; i++)
        th[i] = os.setTimeout(() => {
            console.log('!!!! cool test');
        }, 2000);
}

//test_timer();

//ru.setTimeout(()=> {
//    console.log('hi i am trigger');
//}, 1000);
//
let setTimeout = qruff.setTimeout;
console.log(Object.keys(qruff));
console.log(qruff.CONST_16);

let a = setTimeout(() => {
    console.log('Cool Qruff timer 5000ms be triggerd');
}, 5000);

let b = os.setTimeout(() => {}, 2000);
//console.log('time a is', typeof a);
//console.log('time b is', typeof b);
//console.log('time a is', Object.keys(a));
//console.log('time a is', a.constructor.name);
//console.log('time b is', b.constructor.name);
let c = setTimeout(() => {
    console.log('Cool Qruff timer 2000ms be triggerd');
    qruff.clearTimeout(a);
}, 2000);

