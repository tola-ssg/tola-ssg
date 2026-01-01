#let base(body) = {
  // ==========================================================================
  // Show Rules: Figures & Tables
  // ==========================================================================

  let inside-figure = state("inside-figure", false)
  show figure: it => {
    inside-figure.update(true)
    it
    inside-figure.update(false)
  }

  set table(stroke: white, inset: 10pt)
  show table: set text(fill: blue, size: 12pt)

  show table: it => context {
    if not inside-figure.get() {
      html.div(class: "my-4 mx-2 sm:mx-6")[#html.frame(it)]
    } else { it }
  }
  show figure: it => html.figure(class: "m-4 w-fit mx-auto")[#html.frame(it)]

  // ==========================================================================
  // Show Rules: Math
  // ==========================================================================

  show math.equation: set text(font: "Luciole Math")
  show math.equation.where(block: false): set text(size: 12pt)
  show math.equation.where(block: true): set text(weight: "bold", size: 16pt)

  show math.equation.where(block: false): it => if not inside-figure.get() {
    html.span(class: "inline-block", role: "math")[#html.frame(it)]
  } else { it }

  show math.equation.where(block: true): it => if not inside-figure.get() {
    html.figure(class: "m-4 w-fit mx-auto", role: "math")[#html.frame(it)]
  } else { it }

  html.elem("html", attrs: (lang: "en"))[
    #html.elem("head")[
      #html.elem("title")[Test]
    ]
    #html.elem("body")[
      #body
    ]
  ]
}
