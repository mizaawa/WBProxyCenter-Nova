import {readdir,readFile} from 'node:fs/promises'
import {join,relative} from 'node:path'
const root=new URL('../src/',import.meta.url);import {fileURLToPath} from 'node:url';const dir=fileURLToPath(root);let bad=false
async function walk(base){for(const e of await readdir(base,{withFileTypes:true})){const p=join(base,e.name);if(e.isDirectory())await walk(p);else if(/\.(css|vue)$/.test(p)&&e.name!=='tokens.css'){const s=await readFile(p,'utf8');if(/#[\da-f]{3,8}\b/i.test(s)){console.error(relative(dir,p)+': 颜色必须来自 tokens.css');bad=true}}}}await walk(dir);if(bad)process.exitCode=1
