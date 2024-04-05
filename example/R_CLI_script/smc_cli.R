library(argparser)
library(redist)
library(sf)
library(dplyr)
library(ggplot2)

p <- arg_parser("This is a basic CLI app for generating SMC plans.")

# TAGS FOR THE REDIST_MAP CALL
p <- add_argument(p,
  "--shapefile",
  help = "Enter the name of the shapefile (must be in current directory and cannot include \"./\")",
  type = "character"
)
p <- add_argument(p,
  "--pop-col",
  help = "Enter the name of the population column within the shapefile",
  default = "TOTPOP",
  type = "character"
)
p <- add_argument(p,
  "--n_dists",
  help = "Enter the number of districts for the redistricting.",
  type = "integer"
)
p <- add_argument(p,
  "--pop-tol",
  help = "Enter the allowable population deviance [between 0 and 1]",
  default = 0.01,
  type = "double"
)
p <- add_argument(p,
  "--pop-bounds",
  help = "Enter the population bounds with formatting (lower, target, upper)",
  default = NULL,
  type = "integer",
  nargs = 3
)

# TAGS FOR THE REDIST_SMC CALL
p <- add_argument(p,
  "--n-sims",
  help = "Enter the number of simulations to draw from",
  default = 1000,
  type = "integer"
)
p <- add_argument(p,
  "--compactness",
  help = "Enter the compactness measure for the generated districts",
  default = 1.0,
  type = "double"
)
p <- add_argument(p,
  "--resample",
  help = "Including this flag will set the resampling to true",
  flag = TRUE
)
p <- add_argument(p,
  "--adapt-k-thresh",
  help = "Enter the threshold value used in teh heuristic to select a value ki for each splitting iteration",
  default = 0.985,
  type = "double"
)
p <- add_argument(p,
  "--seq-alpha",
  help = "Enter the amount to adjust the weights by at each resampling step.", default = 0.5, type = "double"
)
p <- add_argument(p,
  "--pop-temper",
  help = "Enter the strength of the automatic population tempering",
  default = 0.0,
  type = "double"
)
p <- add_argument(p,
  "--final-infl",
  help = "Enter the multiplier for the population constraint",
  default = 1,
  type = "double"
)
p <- add_argument(p,
  "--est-label-mult",
  help = "Enter the multiplier for the number of importance samples",
  default = 1.0,
  type = "double"
)
p <- add_argument(p,
  "--verbose",
  help = "Including this flag will create.",
  flag = TRUE
)
p <- add_argument(p,
  "--silent",
  help = "Including this flag will suppress all information while sampling.",
  flag = TRUE
)


# OTHER FLAGS FOR DATA PROCESSING AND REPRODUCIBILITY
p <- add_argument(p,
  "--rng-seed",
  help = "Enter the rng seed for the run",
  default = 42,
  type = "integer"
)
p <- add_argument(p,
  "--tally-cols",
  help = "Enter the names of the columns that you would like to tally",
  default = NULL,
  type = "character",
  nargs = "+"
)
p <- add_argument(p,
  "--output-file",
  help = "Enter the name of the output file.",
  default = "./test_output.csv",
  type = "character"
)
p <- add_argument(p,
  "--print",
  help = "Print the output to the console",
  flag = TRUE
)

argv <- parse_args(p)

vtds <- st_read(dsn = paste0(argv$shapefile))

population <- sum(vtds[[argv$pop_col]])

if (is.null(argv$pop_bounds) || length(argv$pop_bounds) != 3) {
  argv$pop_bounds <- NULL
}

seed <- redist_map(vtds,
  pop_tol = argv$pop_tol,
  total_pop = argv$pop_col,
  ndists = argv$n_dists,
  pop_bounds = argv$pop_bounds
)

set.seed(argv$rng_seed)
plans <- redist_smc(seed,
  nsims = argv$n_sims,
  compactness = argv$compactness,
  resample = argv$resample,
  adapt_k_thresh = argv$adapt_k_thresh,
  seq_alpha = argv$seq_alpha,
  pop_temper = argv$pop_temper,
  final_infl = argv$final_infl,
  est_label_mult = argv$est_label_mult,
  verbose = argv$verbose,
  silent = argv$silent
)


if (!is.null(argv$tally_cols) && is.character(argv$tally_cols)) {
  tally_list <- strsplit(argv$tally_cols, ",")[[1]]
  for (thing in tally_list) {
    plans <- plans %>%
      mutate(tally_var(seed, !!rlang::sym(thing)))
  }
}


if (argv$print) {
  cat("\nNow printing the plans:\n")

  plans <- t(as.matrix(plans))
  apply(plans, 1, function(row) {
    cat(paste0("[", paste(row, collapse = ","), "]", "\n"))
  })
  invisible(NULL)
} else {
  file_name <- argv$output_file
  dir.create(dirname(file_name), recursive = TRUE, showWarnings = FALSE)
  write.csv(plans, file_name)
  write.csv(t(as.matrix(plans)), paste0(tools::file_path_sans_ext(file_name), "_assignments.csv"))
}
