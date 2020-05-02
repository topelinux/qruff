import * as std from "std";
import * as os from "os";
import * as qruff from "qruff";
import { assert } from "./assert.js";

console.log('after import assert');

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

let setTimeout = qruff.setTimeout;

let a = setTimeout(() => {
    assert(false, 'Timer should be caneled');
    console.log('Cool Qruff timer 5000ms be triggerd');
}, 5000);

let c = setTimeout(() => {
    console.log('Cool Qruff timer 2000ms be triggerd');
    qruff.clearTimeout(a);
}, 2000);

