# fluxen send tools — ported from fluxora's x.nu. Gateway-side plumbing
# (rpk/kafka, /admin/send, watch-message) is dropped: fluxen's stage mirror
# is the only receiver. Use as a module:
#   nu -c 'use examples/fluxen.nu *; send 02.concat.yaml'

const BASE = 'http://127.0.0.1:3002'
const ROOT = path self ..    # repo root (this file lives in examples/)
const YAML = $ROOT | path join examples/yaml

def "fluxen file" [] {
    (ls ($YAML | path join "*.yaml") | get name) ++ (ls ($ROOT | path join examples/kdl "*.kdl") | get name)
}

export def send [
    file: string@"fluxen file"    # path, or a name under examples/yaml/
    --patch(-p): any              # record deep-merged over the doc (yaml only)
] {
    let f = if ($file | path exists) { $file | path expand } else {
        $YAML | path join $file
    }
    let ext = $f | path parse | get extension
    match $ext {
        "kdl" => {
            if ($patch | is-empty) {
                open --raw $f | http post --content-type "text/plain" $"($BASE)/send"
            } else {
                print -e "patch needs yaml"; exit 1
            }
        }
        "yaml" | "yml" => {
            let content = open $f
            let content = if ($patch | is-empty) { $content } else {
                $content | merge deep $patch
            }
            $content | to yaml | http post --content-type "text/plain" $"($BASE)/send?fmt=yaml"
        }
        _ => { print -e $"unsupported extension: ($ext)"; exit 1 }
    }
}

# cycle the rack's default row border colour (x.nu:252 port).
# Builds the frame in-process with a cell-path update — deep-merging lists
# through `merge deep` is positional and lossy here; update is exact.
def flashing-frame [colour: string] {
    let f = $YAML | path join "00.chat_layout.yaml"
    open $f | update data.children.1.children.1.children.0.children.0.item.1.attrs.class [
        "nogrow" "as-stretch" "box" "border" "round" "shadow" $colour
    ]
}

export def border-flashing [] {
    for _ in 1.. {
        for i in [primary disable secondary accent] {
            sleep 0.2sec
            flashing-frame $i | to yaml | http post --content-type "text/plain" $"($BASE)/send?fmt=yaml"
        }
    }
}

export def message-concat [] {
    for _ in 1.. {
        send "02.concat.yaml"
        sleep 0.8sec
    }
}

export def message-replace [] {
    for _ in 1.. {
        send "02.replace.yaml"
        sleep 0.8sec
    }
}
