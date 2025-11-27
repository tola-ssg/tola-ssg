#let tola-minimal-conf(
  body
) = [
  #let inside-figure = state("inside-figure", false)

  #show figure: it => {
    inside-figure.update(true)
    it
    inside-figure.update(false)
  }

  #show math.equation.where(block: false): it => if not inside-figure.get() {
    html.elem("span", attrs: (class: "inline-block", role: "math"), html.frame(it))
  } else {it}

  #show math.equation.where(block: true): it => if not inside-figure.get() {
    html.elem("figure", attrs: (class: "w-fit mx-auto", role: "math"), html.frame(it))
  } else {it}

  // #show math.equation: set text(
  //   font: "Luciole Math"
  // )

  #doc
]

#let tola-conf(
  kind: "",
  body
) = [
  #if kind == "minimal" {
    tola-minimal-conf(body)
  }
]

