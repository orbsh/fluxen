export def start [--dev] {
    do {
        cd crates/ui_leptos/
        if $dev {
            trunk serve
        } else {
            trunk build
        }
    }
    cargo run -p stage -- serve
}


# fluxen send tools — ported from fluxora's x.nu. Gateway-side plumbing
# (rpk/kafka, /admin/send, watch-message) is dropped: fluxen's stage mirror
# is the only receiver. Use as a module:
#   nu -c 'use x.nu *; send 02.concat.yaml'

const BASE = 'http://127.0.0.1:3002'
const ROOT = path self .    # repo root (this file lives at the root)
const YAML = $ROOT | path join examples/yaml

def "fluxen file" [] {
    glob examples/**/*.{yaml,yml,json}
    | each { $in | path relative-to $env.PWD }
}

export def send [
    file: string@"fluxen file"    # path, or a name under examples/yaml/
    --patch(-p): any              # record deep-merged over the doc
] {
    let f = if ($file | path exists) { $file | path expand } else {
        $YAML | path join $file
    }
    let ext = $f | path parse | get extension
    match $ext {
        "yaml" | "yml" => {
            let content = open $f
            let content = if ($patch | is-empty) { $content } else {
                $content | merge deep $patch
            }
            $content | to yaml | http post --content-type "text/plain" $"($BASE)/send"
        }
        "json" => {
            open --raw $f | http post --content-type "application/json" $"($BASE)/send"
        }
        _ => { print -e $"unsupported extension: ($ext)"; exit 1 }
    }
}

# cycle the rack's default row border colour via an O(1) layout-root patch
# (ADR 0005): replace only that class array, never re-send the tree.
def flashing-frame [colour: string] {
    {
        action: patch
        event: ""
        path: "/children/1/children/1/children/0/children/0/item/1/attrs/class"
        op: replace
        value: ["nogrow" "as-stretch" "box" "border" "round" "shadow" $colour]
    }
}

export def border-flashing [] {
    for _ in 1.. {
        for i in [primary disable secondary accent] {
            sleep 0.02sec
            flashing-frame $i | to yaml | http post --content-type "text/plain" $"($BASE)/send"
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
